use crate::detect::command_exists;
use crate::error::{MorphError, MorphResult};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

pub fn run(args: &[String]) -> MorphResult<()> {
    let target = args.first().map(String::as_str).unwrap_or("all");
    match target {
        "c" => run_c_demo(),
        "python" => run_python_demo(),
        "all" => {
            run_c_demo()?;
            run_python_demo()
        }
        other => Err(MorphError::InvalidArguments(format!(
            "unknown demo {other:?}; choose c, python, or all"
        ))),
    }
}

fn run_c_demo() -> MorphResult<()> {
    if !cfg!(any(unix, windows)) {
        return Err(MorphError::Unsupported(
            "the bundled C demo requires the Unix or Windows dynamic-loader host".to_string(),
        ));
    }
    let compiler = ["cc", "gcc", "clang", "g++", "zig"]
        .iter()
        .find(|candidate| command_exists(candidate))
        .ok_or_else(|| {
            MorphError::Tool("C demo requires cc, gcc, clang, g++, or zig cc".to_string())
        })?;
    let root = demo_root("c");
    reset_dir(&root)?;
    fs::write(
        root.join("morph_reload.h"),
        include_str!("../sdk/c/morph_reload.h"),
    )?;
    let source_template = include_str!("../demos/c/plugin/plugin.c");
    let first_source = set_demo_version(source_template, 1)?;
    fs::write(root.join("plugin.c"), first_source)?;
    let extension = if cfg!(target_os = "windows") {
        "dll"
    } else if cfg!(target_os = "macos") {
        "dylib"
    } else {
        "so"
    };
    let first_library = root.join(format!("libmorph_demo_v1.{extension}"));
    compile_c_plugin(compiler, &root, &first_library)?;
    let control = root.join("control.request");
    let status = root.join("reload.status");
    let first_library_string = first_library.to_string_lossy().into_owned();
    write_atomic(&control, &first_library_string)?;
    let executable = std::env::current_exe()?;
    let host_args = vec![
        "internal-c-host".to_string(),
        "--library".to_string(),
        first_library.to_string_lossy().into_owned(),
        "--control".to_string(),
        control.to_string_lossy().into_owned(),
        "--status".to_string(),
        status.to_string_lossy().into_owned(),
    ];
    let host = Command::new(&executable)
        .args(&host_args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| MorphError::Child(format!("cannot start C demo host: {error}")))?;
    let host = ChildGuard::new(host);
    wait_for_c_status(&status, 1)?;

    let second_source = set_demo_version(source_template, 2)?;
    fs::write(root.join("plugin.c"), second_source)?;
    let second_library = root.join(format!("libmorph_demo_v2.{extension}"));
    compile_c_plugin(compiler, &root, &second_library)?;
    let second_library_string = second_library.to_string_lossy().into_owned();
    write_atomic(&control, &second_library_string)?;
    wait_for_c_status(&status, 2)?;

    let malformed_source = root.join("malformed_plugin.c");
    fs::write(
        &malformed_source,
        "int morph_wrong_symbol(void) { return 7; }\n",
    )?;
    let malformed_library = root.join(format!("libmorph_demo_bad.{extension}"));
    compile_c_source(compiler, &root, &malformed_library, "malformed_plugin.c")?;
    let malformed_library_string = malformed_library.to_string_lossy().into_owned();
    write_atomic(&control, &malformed_library_string)?;
    wait_for_c_status_error(&status, 3)?;
    thread::sleep(Duration::from_millis(450));
    write_atomic(&control, "STOP")?;
    let output = host
        .wait_with_output()
        .map_err(|error| MorphError::Child(format!("C demo host failed: {error}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print_demo_output("C", &stdout, &stderr);
    if !output.status.success()
        || !stdout.contains("MORPH_C_RELOAD")
        || !stdout.contains("version=1")
        || stdout.matches("value=version=2").count() < 2
        || !stderr.contains("MORPH_C_RELOAD_FAILED")
    {
        return Err(MorphError::Backend(
            "C demo did not show an in-process generation switch".to_string(),
        ));
    }
    println!(
        "[demo:c] PASS — successful reloads changed generations and an incompatible library was rejected"
    );
    Ok(())
}

fn run_python_demo() -> MorphResult<()> {
    let interpreter = if command_exists("python3") {
        "python3"
    } else if command_exists("python") {
        "python"
    } else {
        return Err(MorphError::Tool(
            "Python demo requires python3 or python".to_string(),
        ));
    };
    let root = demo_root("python");
    reset_dir(&root)?;
    fs::write(
        root.join("morph_runtime.py"),
        include_str!("../sdk/python/morph_runtime.py"),
    )?;
    fs::write(root.join("app.py"), include_str!("../demos/python/app.py"))?;
    let logic = root.join("app_logic.py");
    let logic_template = include_str!("../demos/python/app_logic.py");
    fs::write(&logic, logic_template)?;
    let control = root.join("control.request");
    let status = root.join("reload.status");
    write_atomic(&control, "READY")?;
    let worker = Command::new(interpreter)
        .arg("app.py")
        .current_dir(&root)
        .env("MORPH_PYTHON_CONTROL", &control)
        .env("MORPH_PYTHON_STATUS", &status)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| MorphError::Child(format!("cannot start Python demo: {error}")))?;
    let worker = ChildGuard::new(worker);
    wait_for_status(&status, 0)?;
    fs::write(
        &logic,
        logic_template.replace("MORPH_RELOAD_VERSION = 1", "MORPH_RELOAD_VERSION = 2"),
    )?;
    write_atomic(&control, "RELOAD 1")?;
    wait_for_status(&status, 1)?;
    thread::sleep(Duration::from_millis(350));
    fs::write(&logic, "def render(state):\n    return (\n")?;
    write_atomic(&control, "RELOAD 2")?;
    wait_for_status_error(&status, 2)?;
    thread::sleep(Duration::from_millis(350));
    write_atomic(&control, "STOP")?;
    let output = worker
        .wait_with_output()
        .map_err(|error| MorphError::Child(format!("Python demo worker failed: {error}")))?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    print_demo_output("Python", &stdout, &stderr);
    if !output.status.success()
        || !stdout.contains("PYTHON_RELOAD")
        || stdout.matches("python-v2").count() < 2
        || stdout.contains("python-v3")
    {
        return Err(MorphError::Backend(
            "Python demo did not show a controlled module swap".to_string(),
        ));
    }
    println!(
        "[demo:python] PASS — a valid swap applied and an invalid candidate left the last working module active"
    );
    Ok(())
}

fn compile_c_plugin(compiler: &str, root: &Path, output: &Path) -> MorphResult<()> {
    compile_c_source(compiler, root, output, "plugin.c")
}

fn set_demo_version(source: &str, version: u32) -> MorphResult<String> {
    let mut replaced = false;
    let lines = source
        .lines()
        .map(|line| {
            if line.trim_start().starts_with("#define MORPH_DEMO_VERSION ") {
                replaced = true;
                format!("#define MORPH_DEMO_VERSION {version}")
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>();
    if !replaced {
        return Err(MorphError::Config(
            "C demo plugin is missing MORPH_DEMO_VERSION".to_string(),
        ));
    }
    Ok(format!("{}\n", lines.join("\n")))
}

fn compile_c_source(compiler: &str, root: &Path, output: &Path, source: &str) -> MorphResult<()> {
    let output_string = output.to_string_lossy().into_owned();
    let mut arguments = vec!["-O0".to_string()];
    if compiler == "zig" {
        arguments.insert(0, "cc".to_string());
    }
    if cfg!(target_os = "macos") {
        arguments.push("-dynamiclib".to_string());
    } else {
        arguments.push("-shared".to_string());
        if !cfg!(windows) {
            arguments.push("-fPIC".to_string());
        }
    }
    arguments.extend([
        "-I".to_string(),
        ".".to_string(),
        "-o".to_string(),
        output_string,
        source.to_string(),
    ]);
    let result = Command::new(compiler)
        .args(&arguments)
        .current_dir(root)
        .output()
        .map_err(|error| MorphError::Tool(format!("cannot start {compiler}: {error}")))?;
    if !result.status.success() {
        return Err(MorphError::Build(format!(
            "C demo compilation failed: {}",
            String::from_utf8_lossy(&result.stderr).trim()
        )));
    }
    Ok(())
}

fn wait_for_status(path: &Path, generation: u64) -> MorphResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            if source.contains(&format!("\"generation\": {generation}"))
                && source.contains("\"status\": \"ok\"")
            {
                return Ok(());
            }
            if source.contains("\"status\": \"error\"") {
                return Err(MorphError::Backend(source));
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "Python Runtime did not acknowledge generation {generation}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_status_error(path: &Path, generation: u64) -> MorphResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            if source.contains(&format!("\"generation\": {generation}"))
                && source.contains("\"status\": \"error\"")
            {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "Python Runtime did not reject generation {generation}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_c_status(path: &Path, generation: u64) -> MorphResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            if source.contains(&format!("\"generation\": {generation}"))
                && source.contains("\"status\": \"ok\"")
            {
                return Ok(());
            }
            if source.contains(&format!("\"generation\": {generation}"))
                && source.contains("\"status\": \"error\"")
            {
                return Err(MorphError::Backend(source));
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "C host did not acknowledge generation {generation}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn wait_for_c_status_error(path: &Path, generation: u64) -> MorphResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            if source.contains(&format!("\"generation\": {generation}"))
                && source.contains("\"status\": \"error\"")
            {
                return Ok(());
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "C host did not reject generation {generation}"
            )));
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn print_demo_output(name: &str, stdout: &str, stderr: &str) {
    println!("--- {name} demo output ---");
    for line in stdout.lines().take(40) {
        println!("{line}");
    }
    if !stderr.trim().is_empty() {
        eprintln!("--- {name} demo diagnostics ---\n{}", stderr.trim());
    }
    println!("--- end {name} demo output ---");
}

fn demo_root(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("morph-demo-{name}-{}", std::process::id()))
}

fn reset_dir(path: &Path) -> MorphResult<()> {
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)?;
    Ok(())
}

fn write_atomic(path: &Path, value: &str) -> MorphResult<()> {
    crate::file_ops::write_atomic(path, value.as_bytes())?;
    Ok(())
}

struct ChildGuard(Option<Child>);

impl ChildGuard {
    fn new(child: Child) -> Self {
        Self(Some(child))
    }

    fn wait_with_output(mut self) -> std::io::Result<std::process::Output> {
        self.0
            .take()
            .expect("child guard must contain a child")
            .wait_with_output()
    }
}

impl Drop for ChildGuard {
    fn drop(&mut self) {
        if let Some(child) = self.0.as_mut() {
            if child.try_wait().ok().flatten().is_none() {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
    }
}

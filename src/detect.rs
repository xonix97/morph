use crate::config::MorphConfig;
use crate::error::{MorphError, MorphResult};
use crate::file_ops::external_path;
use crate::model::{BackendInfo, BackendKind, BuildSystem, Language, ProjectProfile, ReloadMode};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const IGNORED_DIRS: &[&str] = &[
    ".git",
    ".morph",
    ".venv",
    "__pycache__",
    "target",
    "node_modules",
    "build",
    "dist",
    ".idea",
    ".vscode",
];

pub fn detect_project(path: &Path) -> MorphResult<ProjectProfile> {
    let root = external_path(
        &path
            .canonicalize()
            .map_err(|error| MorphError::Detection(format!("{}: {error}", path.display())))?,
    );
    if !root.is_dir() {
        return Err(MorphError::Detection(format!(
            "{} is not a directory",
            root.display()
        )));
    }
    let files = collect_files(&root, 5)?;
    let config = MorphConfig::load(&root)?;
    let mut languages = Vec::new();
    let mut build_systems = Vec::new();
    let mut manifests = Vec::new();
    let mut frameworks = Vec::new();

    for file in &files {
        let relative = file.strip_prefix(&root).unwrap_or(file);
        let name = relative
            .file_name()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        let extension = relative
            .extension()
            .and_then(|value| value.to_str())
            .unwrap_or_default();
        match name {
            "Cargo.toml" => {
                add_once(&mut languages, Language::Rust);
                add_once(&mut build_systems, BuildSystem::Cargo);
                manifests.push(relative.to_path_buf());
            }
            "go.mod" => {
                add_once(&mut languages, Language::Go);
                add_once(&mut build_systems, BuildSystem::GoModules);
                manifests.push(relative.to_path_buf());
            }
            "pyproject.toml" => {
                add_once(&mut languages, Language::Python);
                add_once(&mut build_systems, BuildSystem::PyProject);
                manifests.push(relative.to_path_buf());
            }
            "requirements.txt" => {
                add_once(&mut languages, Language::Python);
                add_once(&mut build_systems, BuildSystem::PipRequirements);
                manifests.push(relative.to_path_buf());
            }
            "package.json" => {
                add_once(&mut languages, Language::JavaScript);
                add_once(&mut build_systems, BuildSystem::Npm);
                manifests.push(relative.to_path_buf());
                if let Ok(source) = fs::read_to_string(file) {
                    if source.contains("typescript") || source.contains("ts-node") {
                        add_once(&mut languages, Language::TypeScript);
                    }
                    for (needle, framework) in [
                        ("next", "Next.js"),
                        ("react", "React"),
                        ("vite", "Vite"),
                        ("svelte", "Svelte"),
                        ("vue", "Vue"),
                    ] {
                        if source.contains(needle) {
                            add_once(&mut frameworks, framework.to_string());
                        }
                    }
                }
            }
            "tsconfig.json" => {
                add_once(&mut languages, Language::TypeScript);
                manifests.push(relative.to_path_buf());
            }
            "CMakeLists.txt" => {
                add_once(&mut build_systems, BuildSystem::CMake);
                manifests.push(relative.to_path_buf());
            }
            "Makefile" | "makefile" => {
                add_once(&mut build_systems, BuildSystem::Make);
                manifests.push(relative.to_path_buf());
            }
            _ => {}
        }
        match extension {
            "c" | "h" => add_once(&mut languages, Language::C),
            "cc" | "cpp" | "cxx" | "hpp" => add_once(&mut languages, Language::Cpp),
            "py" => add_once(&mut languages, Language::Python),
            "js" | "jsx" | "mjs" | "cjs" => add_once(&mut languages, Language::JavaScript),
            "ts" | "tsx" => add_once(&mut languages, Language::TypeScript),
            "rs" => add_once(&mut languages, Language::Rust),
            "go" => add_once(&mut languages, Language::Go),
            _ => {}
        }
    }

    let explicit_backend = config.backend.as_deref().map(parse_backend).transpose()?;
    let (backend, confidence, ambiguous, limitations) =
        select_backend(&root, &languages, &build_systems, &config, explicit_backend);

    let mut project_roots = vec![root.clone()];
    for manifest in &manifests {
        if let Some(parent) = manifest.parent() {
            let candidate = root.join(parent);
            if !project_roots.contains(&candidate) {
                project_roots.push(candidate);
            }
        }
    }
    let mut runtimes = Vec::new();
    for tool in [
        "python3", "python", "cargo", "go", "node", "cc", "gcc", "clang",
    ] {
        if command_exists(tool) {
            runtimes.push(tool.to_string());
        }
    }

    Ok(ProjectProfile {
        root,
        project_roots,
        languages,
        build_systems,
        manifests,
        frameworks,
        runtimes,
        backend,
        confidence,
        ambiguous,
        limitations,
    })
}

fn select_backend(
    root: &Path,
    languages: &[Language],
    build_systems: &[BuildSystem],
    config: &MorphConfig,
    explicit_backend: Option<BackendKind>,
) -> (BackendInfo, u8, bool, Vec<String>) {
    let has_c = languages.contains(&Language::C);
    let has_python = languages.contains(&Language::Python);
    let mut limitations = Vec::new();
    let (kind, confidence, ambiguous) = if let Some(kind) = explicit_backend {
        (kind, 98, false)
    } else if has_c && config.has_c_integration() && !has_python {
        (BackendKind::CNative, 92, false)
    } else if has_python && config.has_python_integration() && !has_c {
        (BackendKind::PythonControlled, 92, false)
    } else {
        if languages.len() > 1 {
            limitations.push(format!(
                "multiple languages detected ({}); add morph.toml to select a safe integration",
                languages
                    .iter()
                    .map(ToString::to_string)
                    .collect::<Vec<_>>()
                    .join(", ")
            ));
        }
        if has_c {
            limitations.push(
                "ordinary C executables are not reloadable without a Morph SDK host boundary"
                    .to_string(),
            );
        }
        if has_python {
            limitations.push(
                "Python module swapping requires morph_runtime.Runtime and an explicit module allowlist"
                    .to_string(),
            );
        }
        if build_systems.is_empty() {
            limitations.push("no build manifest was found".to_string());
        }
        (BackendKind::DetectionOnly, 45, languages.len() > 1)
    };

    let backend = match kind {
        BackendKind::CNative => c_backend_info(root, config, &mut limitations),
        BackendKind::PythonControlled => python_backend_info(root, config, &mut limitations),
        BackendKind::DetectionOnly => detection_only_info(languages, &limitations),
    };
    (backend, confidence, ambiguous, limitations)
}

fn c_backend_info(root: &Path, config: &MorphConfig, limitations: &mut Vec<String>) -> BackendInfo {
    let compiler = ["cc", "gcc", "clang", "g++", "zig"]
        .iter()
        .find(|tool| command_exists(tool));
    let mut missing_tools = Vec::new();
    if compiler.is_none() {
        missing_tools.push("cc/gcc/clang/g++ (or zig cc)".to_string());
    }
    if !cfg!(any(unix, windows)) {
        missing_tools.push("a tested Unix or Windows dynamic-loader host".to_string());
    }
    if config.c.build.is_empty() && config.c.plugin_source.is_none() {
        limitations.push("c.plugin_source or c.build must be configured".to_string());
    }
    if config.c.host_command.is_empty() {
        limitations.push(
            "c.host_command is required; the host must use the Morph C ABI and control file"
                .to_string(),
        );
    } else if let Some(program) = config.c.host_command.first() {
        if program != "{morph}" && !command_available_from_root(root, program) {
            limitations.push(format!(
                "configured C host command is unavailable: {program}"
            ));
        }
    }
    let source_available = config
        .c
        .plugin_source
        .as_deref()
        .map(|source| {
            let available = root.join(source).is_file();
            if !available {
                limitations.push(format!("configured plugin source is missing: {source}"));
            }
            available
        })
        .unwrap_or(false);
    let host_available = config
        .c
        .host_command
        .first()
        .map(|program| program == "{morph}" || command_available_from_root(root, program))
        .unwrap_or(false);
    BackendInfo {
        kind: BackendKind::CNative,
        name: "C native generation backend".to_string(),
        mode: ReloadMode::NativeLibraryGeneration,
        available: missing_tools.is_empty()
            && config.has_c_integration()
            && source_available
            && host_available,
        capabilities: vec![
            "rebuilds a shared library into a new generation".to_string(),
            "validates activation through the host ABI".to_string(),
            "preserves host-owned state".to_string(),
            "keeps the last loaded library mapped for safe rollback".to_string(),
        ],
        limitations: vec![
            "does not hot-reload arbitrary executable code".to_string(),
            "the host must avoid callbacks or threads into an old generation".to_string(),
        ],
        missing_tools,
    }
}

fn python_backend_info(
    root: &Path,
    config: &MorphConfig,
    limitations: &mut Vec<String>,
) -> BackendInfo {
    let mut missing_tools = Vec::new();
    if !command_exists("python3") && !command_exists("python") {
        missing_tools.push("python3 (or python)".to_string());
    }
    if config.python.modules.is_empty() {
        limitations.push("python.modules must name explicitly reloadable modules".to_string());
    }
    if config.python.command.is_empty() {
        limitations
            .push("python.command must start the controlled Runtime application".to_string());
    }
    let interpreter_available = if let Some(program) = config.python.command.first() {
        if !command_available_from_root(root, program) {
            limitations.push(format!(
                "configured Python interpreter is unavailable: {program}"
            ));
            false
        } else {
            true
        }
    } else {
        false
    };
    let mut entrypoints_available = true;
    for argument in config.python.command.iter().skip(1) {
        if argument.ends_with(".py") && !root.join(argument).is_file() {
            limitations.push(format!(
                "configured Python entrypoint is missing: {argument}"
            ));
            entrypoints_available = false;
        }
    }
    let imports_available = if interpreter_available && !config.python.imports.is_empty() {
        let imports = config.python.imports.join(", ");
        let script = config
            .python
            .imports
            .iter()
            .map(|name| format!("importlib.import_module({name:?})"))
            .collect::<Vec<_>>()
            .join("; ");
        let interpreter = config.python.command.first().expect("checked above");
        match Command::new(interpreter)
            .args(["-c", &format!("import importlib; {script}")])
            .current_dir(root)
            .output()
        {
            Ok(output) if output.status.success() => true,
            Ok(output) => {
                let detail = summarize_command_error(&output.stderr);
                limitations.push(format!(
                    "configured Python imports are unavailable ({imports}): {}",
                    if detail.is_empty() {
                        "import check failed"
                    } else {
                        &detail
                    }
                ));
                missing_tools.push(format!("Python imports: {imports}"));
                false
            }
            Err(error) => {
                limitations.push(format!(
                    "could not check configured Python imports ({imports}): {error}"
                ));
                missing_tools.push(format!("Python imports: {imports}"));
                false
            }
        }
    } else {
        true
    };
    BackendInfo {
        kind: BackendKind::PythonControlled,
        name: "Python controlled module backend".to_string(),
        mode: ReloadMode::InProcessModuleSwap,
        available: missing_tools.is_empty()
            && config.has_python_integration()
            && interpreter_available
            && entrypoints_available
            && imports_available,
        capabilities: vec![
            "loads candidate modules under a temporary name before activation".to_string(),
            "preserves state held by morph_runtime.Runtime".to_string(),
            "reports reload errors without replacing the last good module".to_string(),
        ],
        limitations: vec![
            "from module import name references are not updated".to_string(),
            "only explicitly allowlisted modules using the Runtime slot API are supported"
                .to_string(),
            "arbitrary global side effects are not rolled back".to_string(),
        ],
        missing_tools,
    }
}

fn summarize_command_error(stderr: &[u8]) -> String {
    let text = String::from_utf8_lossy(stderr);
    text.lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .unwrap_or("import check failed")
        .trim()
        .to_string()
}

fn detection_only_info(languages: &[Language], limitations: &[String]) -> BackendInfo {
    let mut missing_tools = Vec::new();
    if languages.contains(&Language::Python)
        && !command_exists("python3")
        && !command_exists("python")
    {
        missing_tools.push("python3 (or python)".to_string());
    }
    BackendInfo {
        kind: BackendKind::DetectionOnly,
        name: "detection-only backend".to_string(),
        mode: ReloadMode::DetectionOnly,
        available: false,
        capabilities: vec![
            "project and toolchain detection".to_string(),
            "capability reporting".to_string(),
        ],
        limitations: limitations.to_vec(),
        missing_tools,
    }
}

fn parse_backend(value: &str) -> MorphResult<BackendKind> {
    match value {
        "c" | "c-native" => Ok(BackendKind::CNative),
        "python" | "python-controlled" => Ok(BackendKind::PythonControlled),
        "detect" | "detection-only" => Ok(BackendKind::DetectionOnly),
        other => Err(MorphError::Config(format!("unknown backend {other:?}"))),
    }
}

fn collect_files(root: &Path, max_depth: usize) -> MorphResult<Vec<PathBuf>> {
    let mut output = Vec::new();
    collect_files_inner(root, max_depth, 0, &mut output)?;
    Ok(output)
}

fn collect_files_inner(
    directory: &Path,
    max_depth: usize,
    depth: usize,
    output: &mut Vec<PathBuf>,
) -> MorphResult<()> {
    if depth > max_depth {
        return Ok(());
    }
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !IGNORED_DIRS.contains(&name.as_str()) {
                collect_files_inner(&path, max_depth, depth + 1, output)?;
            }
        } else if metadata.is_file() {
            output.push(path);
        }
    }
    Ok(())
}

fn add_once<T: PartialEq>(items: &mut Vec<T>, item: T) {
    if !items.contains(&item) {
        items.push(item);
    }
}

pub fn command_exists(command: &str) -> bool {
    if command.contains(std::path::MAIN_SEPARATOR) {
        return Path::new(command).is_file();
    }
    let Some(path) = std::env::var_os("PATH") else {
        return false;
    };
    let candidates = std::env::split_paths(&path);
    for directory in candidates {
        let candidate = directory.join(command);
        if candidate.is_file() {
            return true;
        }
        #[cfg(windows)]
        if directory.join(format!("{command}.exe")).is_file() {
            return true;
        }
    }
    false
}

fn command_available_from_root(root: &Path, command: &str) -> bool {
    let expanded = command.replace("{root}", &root.to_string_lossy());
    let path = Path::new(&expanded);
    if path.is_absolute()
        || expanded.contains(std::path::MAIN_SEPARATOR)
        || expanded.starts_with('.')
    {
        let candidate = if path.is_absolute() {
            path.to_path_buf()
        } else {
            root.join(path)
        };
        candidate.is_file()
    } else {
        command_exists(command)
    }
}

pub fn run_tool_version(command: &str) -> String {
    match Command::new(command).arg("--version").output() {
        Ok(output) => {
            let text = String::from_utf8_lossy(&output.stdout);
            let error = String::from_utf8_lossy(&output.stderr);
            let value = if text.trim().is_empty() { error } else { text };
            value
                .lines()
                .next()
                .unwrap_or("available")
                .trim()
                .to_string()
        }
        Err(error) => format!("unavailable: {error}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn detects_representative_manifests() {
        let root = std::env::temp_dir().join(format!(
            "morph-detect-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
        fs::write(root.join("main.rs"), "fn main() {}\n").unwrap();
        fs::write(
            root.join("package.json"),
            "{\"devDependencies\":{\"typescript\":\"1\"}}\n",
        )
        .unwrap();
        let profile = detect_project(&root).unwrap();
        assert!(profile.languages.contains(&Language::Rust));
        assert!(profile.languages.contains(&Language::JavaScript));
        assert!(profile.ambiguous);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn detects_all_requested_language_signals_without_selecting_randomly() {
        let root = std::env::temp_dir().join(format!(
            "morph-detect-all-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Cargo.toml"), "[package]\nname=\"demo\"\n").unwrap();
        fs::write(root.join("pyproject.toml"), "[project]\nname=\"demo\"\n").unwrap();
        fs::write(root.join("package.json"), "{\"name\":\"demo\"}\n").unwrap();
        fs::write(root.join("go.mod"), "module example.test/demo\n").unwrap();
        fs::write(root.join("plugin.c"), "int main(void) { return 0; }\n").unwrap();
        fs::write(root.join("component.ts"), "export const value = 1;\n").unwrap();
        fs::write(root.join("script.py"), "print(1)\n").unwrap();
        fs::write(root.join("main.go"), "package main\n").unwrap();
        let profile = detect_project(&root).unwrap();
        assert!(profile.languages.contains(&Language::C));
        assert!(profile.languages.contains(&Language::Rust));
        assert!(profile.languages.contains(&Language::Python));
        assert!(profile.languages.contains(&Language::JavaScript));
        assert!(profile.languages.contains(&Language::TypeScript));
        assert!(profile.languages.contains(&Language::Go));
        assert!(profile.ambiguous);
        assert_eq!(profile.backend.kind, BackendKind::DetectionOnly);
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn explicit_python_integration_selects_python_backend() {
        let root = std::env::temp_dir().join(format!(
            "morph-detect-python-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("app.py"), "print(1)\n").unwrap();
        fs::write(
            root.join("morph.toml"),
            "[morph]\nbackend=\"python-controlled\"\n[python]\ncommand=[\"python3\",\"app.py\"]\nmodules=[\"app\"]\n",
        )
        .unwrap();
        let profile = detect_project(&root).unwrap();
        assert_eq!(profile.backend.kind, BackendKind::PythonControlled);
        assert_eq!(profile.confidence, 98);
        fs::remove_dir_all(root).unwrap();
    }
}

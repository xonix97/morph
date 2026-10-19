use crate::backend::{Backend, BackendContext, BackendSession, ReloadReport, ReloadStatus};
use crate::config::CConfig;
use crate::error::{MorphError, MorphResult};
use crate::file_ops::external_path;
use crate::watch::configured_watch_paths;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

pub struct CBackend;

impl Backend for CBackend {
    fn id(&self) -> &'static str {
        "c-native"
    }

    fn start(&self, context: BackendContext) -> MorphResult<Box<dyn BackendSession>> {
        let session = CSession::start(context)?;
        println!(
            "[c-native] running host boundary; state is owned by the host and library generations stay mapped"
        );
        session.print_startup();
        Ok(Box::new(session))
    }
}

struct CSession {
    root: PathBuf,
    config: CConfig,
    executable: PathBuf,
    control_file: PathBuf,
    status_file: PathBuf,
    current_library: PathBuf,
    generation: u64,
    request_generation: u64,
    child: Child,
    host_pid: u32,
    watch_paths: Vec<PathBuf>,
    verbose: bool,
}

impl CSession {
    fn start(context: BackendContext) -> MorphResult<Self> {
        if context.config.c.host_command.is_empty() {
            return Err(MorphError::Unsupported(
                "C development requires c.host_command and a host using sdk/c/morph_reload.h"
                    .to_string(),
            ));
        }
        let root = context.profile.root.clone();
        let config = context.config.c.clone();
        let morph_dir = root.join(".morph").join("c");
        fs::create_dir_all(&morph_dir)?;
        let control_file = config
            .control_file
            .as_deref()
            .map(|path| root.join(path))
            .unwrap_or_else(|| morph_dir.join("reload.request"));
        let status_file = config
            .status_file
            .as_deref()
            .map(|path| root.join(path))
            .unwrap_or_else(|| morph_dir.join("reload.status"));
        if let Some(parent) = control_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = status_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let _ = fs::remove_file(&status_file);
        let generation = 1;
        let current_library = generation_path(&root, &config, generation);
        build_library(
            &root,
            &config,
            &context.executable,
            &current_library,
            &control_file,
            &status_file,
            context.verbose,
        )?;
        write_control(&control_file, &current_library)?;

        let host_command = expand_command(
            &config.host_command,
            &root,
            &context.executable,
            &current_library,
            &control_file,
            &status_file,
            config.plugin_source.as_deref(),
        );
        let Some((program, arguments)) = host_command.split_first() else {
            return Err(MorphError::Config(
                "c.host_command cannot be empty".to_string(),
            ));
        };
        let mut command = Command::new(program);
        command
            .args(arguments)
            .current_dir(&root)
            .env("MORPH_RELOAD_LIBRARY", &current_library)
            .env("MORPH_RELOAD_CONTROL", &control_file)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            MorphError::Child(format!("cannot start C host {program:?}: {error}"))
        })?;
        let host_pid = child.id();
        if let Err(error) = wait_for_c_status(&status_file, 1, host_pid) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }

        let watch_paths = if config.watch.is_empty() {
            config
                .plugin_source
                .as_deref()
                .map(|source| vec![root.join(source)])
                .unwrap_or_else(|| vec![root.clone()])
        } else {
            configured_watch_paths(&root, &config.watch)
        };
        Ok(Self {
            root,
            config,
            executable: context.executable,
            control_file,
            status_file,
            current_library,
            generation,
            request_generation: generation,
            child,
            host_pid,
            watch_paths,
            verbose: context.verbose,
        })
    }

    fn print_startup(&self) {
        println!(
            "[c-native] loaded generation {} from {}",
            self.generation,
            self.current_library.display()
        );
        println!("[c-native] watching: {}", display_paths(&self.watch_paths));
    }
}

impl BackendSession for CSession {
    fn watch_paths(&self) -> Vec<PathBuf> {
        self.watch_paths.clone()
    }

    fn reload(&mut self, changed: &[PathBuf]) -> MorphResult<ReloadReport> {
        let next_generation = self.request_generation + 1;
        let next_library = generation_path(&self.root, &self.config, next_generation);
        println!(
            "[c-native] change detected in {}; building generation {}",
            display_paths(changed),
            next_generation
        );
        if let Err(error) = build_library(
            &self.root,
            &self.config,
            &self.executable,
            &next_library,
            &self.control_file,
            &self.status_file,
            self.verbose,
        ) {
            eprintln!(
                "[c-native] build failed; keeping generation {}: {error}",
                self.generation
            );
            return Ok(ReloadReport {
                status: ReloadStatus::BuildFailed,
                message: error.to_string(),
                preserved_state: true,
                generation: self.generation,
            });
        }
        if let Err(error) = write_control(&self.control_file, &next_library) {
            eprintln!(
                "[c-native] activation request failed; keeping generation {}: {error}",
                self.generation
            );
            return Ok(ReloadReport {
                status: ReloadStatus::Rejected,
                message: error.to_string(),
                preserved_state: true,
                generation: self.generation,
            });
        }
        self.request_generation = next_generation;
        if let Err(error) = wait_for_c_status(&self.status_file, next_generation, self.host_pid) {
            eprintln!(
                "[c-native] host rejected generation {}; keeping generation {}: {error}",
                next_generation, self.generation
            );
            let rejected = error.to_string().contains("\"status\": \"error\"");
            return Ok(ReloadReport {
                status: if rejected {
                    ReloadStatus::Rejected
                } else {
                    ReloadStatus::Indeterminate
                },
                message: error.to_string(),
                preserved_state: rejected,
                generation: self.generation,
            });
        }
        self.generation = next_generation;
        self.current_library = next_library;
        println!(
            "[c-native] activation requested for generation {}; host validates the ABI before switching",
            self.generation
        );
        Ok(ReloadReport {
            status: ReloadStatus::Applied,
            message: "new library generation sent to the host".to_string(),
            preserved_state: true,
            generation: self.generation,
        })
    }

    fn is_alive(&mut self) -> MorphResult<bool> {
        Ok(self.child.try_wait()?.is_none())
    }

    fn stop(&mut self) {
        let _ = write_control(&self.control_file, Path::new("STOP"));
        std::thread::sleep(std::time::Duration::from_millis(250));
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for CSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn build_library(
    root: &Path,
    config: &CConfig,
    executable: &Path,
    output: &Path,
    control_file: &Path,
    status_file: &Path,
    verbose: bool,
) -> MorphResult<()> {
    let source = config
        .plugin_source
        .as_deref()
        .map(|path| root.join(path))
        .ok_or_else(|| MorphError::Config("c.plugin_source is required".to_string()))?;
    if !source.is_file() {
        return Err(MorphError::Build(format!(
            "plugin source is missing: {}",
            source.display()
        )));
    }
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent)?;
    }
    let command = if config.build.is_empty() {
        let compiler = ["cc", "gcc", "clang", "g++", "zig"]
            .iter()
            .find(|candidate| crate::detect::command_exists(candidate))
            .ok_or_else(|| {
                MorphError::Tool(
                    "cc, gcc, clang, g++, or zig cc is required for C reload".to_string(),
                )
            })?;
        let mut command = if *compiler == "zig" {
            vec!["zig".to_string(), "cc".to_string()]
        } else {
            vec![(*compiler).to_string()]
        };
        if cfg!(target_os = "macos") {
            command.push("-dynamiclib".to_string());
        } else {
            command.push("-shared".to_string());
            if !cfg!(windows) {
                command.push("-fPIC".to_string());
            }
        }
        command.extend([
            "-O0".to_string(),
            "-o".to_string(),
            "{output}".to_string(),
            "{source}".to_string(),
        ]);
        command.splice(
            2..2,
            [
                "-I".to_string(),
                external_path(source.parent().unwrap_or_else(|| Path::new(".")))
                    .to_string_lossy()
                    .into_owned(),
            ],
        );
        let checkout_sdk = external_path(&root.join("../../sdk/c"));
        if checkout_sdk.is_dir() {
            command.splice(
                4..4,
                [
                    "-I".to_string(),
                    checkout_sdk.to_string_lossy().into_owned(),
                ],
            );
        }
        command
    } else {
        config.build.clone()
    };
    let mut command = expand_command(
        &command,
        root,
        executable,
        output,
        control_file,
        status_file,
        config.plugin_source.as_deref(),
    );
    if cfg!(windows) {
        command.retain(|argument| argument != "-fPIC" && argument != "-fpic");
    }
    if command.first().map(String::as_str) == Some("cc") && !crate::detect::command_exists("cc") {
        if crate::detect::command_exists("gcc") {
            command[0] = "gcc".to_string();
        } else if crate::detect::command_exists("clang") {
            command[0] = "clang".to_string();
        } else if crate::detect::command_exists("g++") {
            command[0] = "g++".to_string();
        } else if crate::detect::command_exists("zig") {
            command[0] = "zig".to_string();
            command.insert(1, "cc".to_string());
        }
    }
    let Some((program, arguments)) = command.split_first() else {
        return Err(MorphError::Config("c.build cannot be empty".to_string()));
    };
    let mut process = Command::new(program);
    process.args(arguments).current_dir(external_path(root));
    if verbose {
        println!("[c-native] build: {}", shell_join(&command));
        process.stdout(Stdio::inherit()).stderr(Stdio::inherit());
    }
    let result = process.output().map_err(|error| {
        MorphError::Tool(format!("cannot start C compiler {program:?}: {error}"))
    })?;
    if !result.status.success() {
        let stderr = String::from_utf8_lossy(&result.stderr);
        return Err(MorphError::Build(format!(
            "compiler exited with {}; {}",
            result.status,
            stderr.trim()
        )));
    }
    if !output.is_file() || fs::metadata(output)?.len() == 0 {
        return Err(MorphError::Build(format!(
            "compiler succeeded but did not create {}",
            output.display()
        )));
    }
    Ok(())
}

fn generation_path(root: &Path, config: &CConfig, generation: u64) -> PathBuf {
    let base = config
        .plugin_output
        .as_deref()
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(".morph/c/plugin"));
    let base_name = base
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("plugin");
    let configured_extension = base.extension().and_then(|value| value.to_str());
    let extension =
        if cfg!(target_os = "windows") && matches!(configured_extension, Some("so" | "dylib")) {
            "dll"
        } else if let Some(extension) = configured_extension {
            extension
        } else if cfg!(target_os = "windows") {
            "dll"
        } else if cfg!(target_os = "macos") {
            "dylib"
        } else {
            "so"
        };
    let directory = if base.is_absolute() {
        base.parent()
            .unwrap_or_else(|| Path::new("/"))
            .to_path_buf()
    } else {
        root.join(base.parent().unwrap_or_else(|| Path::new("")))
    };
    directory.join(format!("{base_name}.g{generation}.{extension}"))
}

fn write_control(control: &Path, value: &Path) -> MorphResult<()> {
    if let Some(parent) = control.parent() {
        fs::create_dir_all(parent)?;
    }
    crate::file_ops::write_atomic(control, value.to_string_lossy().as_bytes())?;
    Ok(())
}

fn expand_command(
    command: &[String],
    root: &Path,
    executable: &Path,
    library: &Path,
    control: &Path,
    status: &Path,
    source: Option<&str>,
) -> Vec<String> {
    let root = external_path(root);
    let executable = external_path(executable);
    let library = external_path(library);
    let control = external_path(control);
    let status = external_path(status);
    command
        .iter()
        .map(|part| {
            part.replace("{root}", &root.to_string_lossy())
                .replace("{morph}", &executable.to_string_lossy())
                .replace("{library}", &library.to_string_lossy())
                .replace("{control}", &control.to_string_lossy())
                .replace("{status}", &status.to_string_lossy())
                .replace(
                    "{source}",
                    &root.join(source.unwrap_or("plugin.c")).to_string_lossy(),
                )
                .replace("{output}", &library.to_string_lossy())
        })
        .collect()
}

fn wait_for_c_status(path: &Path, generation: u64, host_pid: u32) -> MorphResult<()> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            let pid = json_number(&source, "pid");
            let actual_generation = json_number(&source, "generation");
            let status = json_string(&source, "status");
            if pid == Some(u64::from(host_pid))
                && actual_generation == Some(generation)
                && status.as_deref() == Some("ok")
            {
                return Ok(());
            }
            if pid == Some(u64::from(host_pid))
                && actual_generation == Some(generation)
                && status.as_deref() == Some("error")
            {
                return Err(MorphError::Backend(source));
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "C host did not acknowledge generation {generation} in {}",
                path.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn json_number(source: &str, key: &str) -> Option<u64> {
    let marker = format!("\"{key}\"");
    let value = source
        .split_once(&marker)?
        .1
        .split_once(':')?
        .1
        .trim_start();
    let digits = value
        .chars()
        .take_while(|character| character.is_ascii_digit())
        .collect::<String>();
    digits.parse().ok()
}

fn json_string(source: &str, key: &str) -> Option<String> {
    let marker = format!("\"{key}\"");
    let value = source
        .split_once(&marker)?
        .1
        .split_once(':')?
        .1
        .trim_start();
    let value = value.strip_prefix('"')?;
    Some(value.split('"').next()?.to_string())
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

fn shell_join(command: &[String]) -> String {
    command
        .iter()
        .map(|part| {
            if part
                .chars()
                .all(|character| character.is_ascii_alphanumeric() || "_./:-".contains(character))
            {
                part.clone()
            } else if cfg!(windows) {
                format!("\"{}\"", part.replace('"', "\\\""))
            } else {
                format!("'{}'", part.replace('\'', "'\\''"))
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn failed_build_does_not_create_a_candidate_library() {
        let root = std::env::temp_dir().join(format!(
            "morph-c-build-{}",
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("plugin.c"), "int plugin = 1;\n").unwrap();
        let (build, argument) = if cfg!(windows) {
            (
                vec!["cmd".to_string(), "/C".to_string(), "exit 1".to_string()],
                "cmd",
            )
        } else {
            (
                vec!["sh".to_string(), "-c".to_string(), "exit 1".to_string()],
                "sh",
            )
        };
        let config = CConfig {
            plugin_source: Some("plugin.c".to_string()),
            build,
            ..CConfig::default()
        };
        assert!(!argument.is_empty());
        let output = root.join("candidate.so");
        let result = build_library(
            &root,
            &config,
            &std::env::current_exe().unwrap(),
            &output,
            &root.join("control"),
            &root.join("status"),
            false,
        );
        assert!(matches!(result, Err(MorphError::Build(_))));
        assert!(!output.exists());
        fs::remove_dir_all(root).unwrap();
    }
}

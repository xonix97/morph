use crate::backend::{Backend, BackendContext, BackendSession, ReloadReport, ReloadStatus};
use crate::config::PythonConfig;
use crate::error::{MorphError, MorphResult};
use crate::file_ops::write_atomic;
use crate::watch::configured_watch_paths;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub struct PythonBackend;

impl Backend for PythonBackend {
    fn id(&self) -> &'static str {
        "python-controlled"
    }

    fn start(&self, context: BackendContext) -> MorphResult<Box<dyn BackendSession>> {
        let session = PythonSession::start(context)?;
        println!(
            "[python-controlled] the application owns state through morph_runtime.Runtime; reload requests are acknowledged by the worker"
        );
        session.print_startup();
        Ok(Box::new(session))
    }
}

struct PythonSession {
    config: PythonConfig,
    control_file: PathBuf,
    status_file: PathBuf,
    session_id: String,
    generation: u64,
    child: Child,
    watch_paths: Vec<PathBuf>,
}

impl PythonSession {
    fn start(context: BackendContext) -> MorphResult<Self> {
        let config = context.config.python.clone();
        let root = context.profile.root.clone();
        let morph_dir = root.join(".morph").join("python");
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
        let _ = fs::remove_file(&control_file);
        let _ = fs::remove_file(&status_file);
        let session_id = format!(
            "{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        write_control(&control_file, &format!("SESSION {session_id}"))?;
        let Some((program, arguments)) = config.command.split_first() else {
            return Err(MorphError::Config(
                "python.command must include an interpreter and entrypoint".to_string(),
            ));
        };
        let mut command = Command::new(program);
        let sdk_path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("sdk/python");
        let python_path = match std::env::var_os("PYTHONPATH") {
            Some(existing) => {
                let mut paths = vec![sdk_path.clone()];
                paths.extend(std::env::split_paths(&existing));
                std::env::join_paths(paths).map_err(|error| {
                    MorphError::Config(format!("cannot construct PYTHONPATH: {error}"))
                })?
            }
            None => std::env::join_paths([sdk_path.clone()]).map_err(|error| {
                MorphError::Config(format!("cannot construct PYTHONPATH: {error}"))
            })?,
        };
        command
            .args(arguments)
            .current_dir(&root)
            .env("MORPH_PYTHON_CONTROL", &control_file)
            .env("MORPH_PYTHON_STATUS", &status_file)
            .env("MORPH_SESSION", &session_id)
            .env("MORPH_PROJECT_ROOT", &root)
            .env("PYTHONPATH", python_path)
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit());
        let mut child = command.spawn().map_err(|error| {
            MorphError::Child(format!(
                "cannot start Python application {program:?}: {error}"
            ))
        })?;
        if let Err(error) = wait_for_status(&status_file, &session_id, 0) {
            let _ = child.kill();
            let _ = child.wait();
            return Err(error);
        }
        let watch_paths = if config.watch.is_empty() {
            vec![root.clone()]
        } else {
            configured_watch_paths(&root, &config.watch)
        };
        Ok(Self {
            config,
            control_file,
            status_file,
            session_id,
            generation: 0,
            child,
            watch_paths,
        })
    }

    fn print_startup(&self) {
        println!(
            "[python-controlled] modules: {}",
            self.config.modules.join(", ")
        );
        println!(
            "[python-controlled] watching: {}",
            display_paths(&self.watch_paths)
        );
    }
}

impl BackendSession for PythonSession {
    fn watch_paths(&self) -> Vec<PathBuf> {
        self.watch_paths.clone()
    }

    fn reload(&mut self, changed: &[PathBuf]) -> MorphResult<ReloadReport> {
        self.generation += 1;
        println!(
            "[python-controlled] change detected in {}; requesting module generation {}",
            display_paths(changed),
            self.generation
        );
        if let Err(error) =
            write_control(&self.control_file, &format!("RELOAD {}", self.generation))
        {
            return Ok(ReloadReport {
                status: ReloadStatus::Rejected,
                message: error.to_string(),
                preserved_state: true,
                generation: self.generation - 1,
            });
        }
        match wait_for_status(&self.status_file, &self.session_id, self.generation) {
            Ok(message) => {
                println!("[python-controlled] {message}");
                Ok(ReloadReport {
                    status: ReloadStatus::Applied,
                    message,
                    preserved_state: true,
                    generation: self.generation,
                })
            }
            Err(error) => {
                eprintln!("[python-controlled] reload was not activated: {error}");
                let acknowledged_error = !error.to_string().contains("no Runtime acknowledgement");
                Ok(ReloadReport {
                    status: if acknowledged_error {
                        ReloadStatus::Rejected
                    } else {
                        ReloadStatus::Indeterminate
                    },
                    message: error.to_string(),
                    preserved_state: acknowledged_error,
                    generation: self.generation - 1,
                })
            }
        }
    }

    fn is_alive(&mut self) -> MorphResult<bool> {
        Ok(self.child.try_wait()?.is_none())
    }

    fn stop(&mut self) {
        let _ = write_control(&self.control_file, "STOP");
        std::thread::sleep(Duration::from_millis(250));
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}

impl Drop for PythonSession {
    fn drop(&mut self) {
        self.stop();
    }
}

fn write_control(path: &PathBuf, value: &str) -> MorphResult<()> {
    write_atomic(path, value.as_bytes())?;
    Ok(())
}

fn wait_for_status(path: &PathBuf, session_id: &str, generation: u64) -> MorphResult<String> {
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        if let Ok(source) = fs::read_to_string(path) {
            let has_session = source.contains(&format!("\"session\": \"{session_id}\""));
            let has_generation = source.contains(&format!("\"generation\": {generation}"));
            if has_session {
                if has_generation && source.contains("\"status\": \"ok\"") {
                    return Ok(format!(
                        "module generation {generation} activated; Runtime state preserved"
                    ));
                }
                if has_generation && source.contains("\"status\": \"error\"") {
                    return Err(MorphError::Backend(source));
                }
            }
        }
        if Instant::now() >= deadline {
            return Err(MorphError::Backend(format!(
                "no Runtime acknowledgement for generation {generation} in {}",
                path.display()
            )));
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn display_paths(paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| path.display().to_string())
        .collect::<Vec<_>>()
        .join(", ")
}

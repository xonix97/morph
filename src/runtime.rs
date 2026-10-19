use crate::backend::{Backend, BackendContext, BackendSession, ReloadStatus};
use crate::c_backend::CBackend;
use crate::config::MorphConfig;
use crate::detect::{detect_project, run_tool_version};
use crate::error::{MorphError, MorphResult};
use crate::model::{BackendKind, BuildSystem, Language, ProjectProfile};
use crate::python_backend::PythonBackend;
use crate::watch::FileWatcher;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn run_cli(args: &[String]) -> MorphResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        print_help();
        return Ok(());
    };
    match command {
        "help" | "--help" | "-h" => {
            print_help();
            Ok(())
        }
        "detect" => run_detect(&args[1..]),
        "doctor" => run_doctor(&args[1..]),
        "backends" => {
            print_backends();
            Ok(())
        }
        "dev" => run_dev(&args[1..]),
        "init" => crate::init::run(&args[1..]),
        "demo" => crate::demo::run(&args[1..]),
        "internal-c-host" => crate::c_host::run(&args[1..]),
        other => Err(MorphError::InvalidArguments(format!(
            "unknown command {other:?}; try `morph help`"
        ))),
    }
}

fn run_detect(args: &[String]) -> MorphResult<()> {
    let (root, verbose) = parse_path_and_verbose(args)?;
    let profile = detect_project(&root)?;
    print_profile(&profile, verbose);
    Ok(())
}

fn run_doctor(args: &[String]) -> MorphResult<()> {
    let (root, verbose) = parse_path_and_verbose(args)?;
    let profile = detect_project(&root)?;
    print_profile(&profile, verbose);
    println!();
    println!("Doctor checks:");
    let mut missing = profile.backend.missing_tools.clone();
    let tools = required_tools(&profile);
    for tool in tools {
        let available = if tool == "cc" {
            crate::detect::command_exists("cc")
                || crate::detect::command_exists("gcc")
                || crate::detect::command_exists("clang")
                || crate::detect::command_exists("g++")
                || crate::detect::command_exists("zig")
        } else if tool == "python3" {
            crate::detect::command_exists("python3") || crate::detect::command_exists("python")
        } else {
            crate::detect::command_exists(tool)
        };
        if available {
            let version_tool = if tool == "cc" && !crate::detect::command_exists("cc") {
                if crate::detect::command_exists("gcc") {
                    "gcc"
                } else if crate::detect::command_exists("clang") {
                    "clang"
                } else if crate::detect::command_exists("g++") {
                    "g++"
                } else {
                    "zig"
                }
            } else if tool == "python3" && !crate::detect::command_exists("python3") {
                "python"
            } else {
                tool
            };
            println!("  [ok]   {tool}: {}", run_tool_version(version_tool));
        } else {
            println!("  [miss] {tool}: not found on PATH");
            if profile
                .backend
                .missing_tools
                .iter()
                .any(|value| value.contains(tool))
                && !missing.iter().any(|value| value == tool)
            {
                missing.push(tool.to_string());
            }
        }
    }
    if profile.backend.available && missing.is_empty() {
        println!(
            "  [ok]   selected backend {} is available",
            profile.backend.kind
        );
        Ok(())
    } else if !missing.is_empty() {
        Err(MorphError::Tool(format!("missing: {}", missing.join(", "))))
    } else {
        Err(MorphError::Unsupported(
            "no safe hot-reload backend selected; add an explicit morph.toml integration"
                .to_string(),
        ))
    }
}

fn run_dev(args: &[String]) -> MorphResult<()> {
    let (root, verbose) = parse_path_and_verbose(args)?;
    let profile = detect_project(&root)?;
    print_profile(&profile, verbose);
    if profile.backend.kind == BackendKind::DetectionOnly {
        return Err(MorphError::Unsupported(
            "detection succeeded, but no safe reload backend applies; see the limitations above"
                .to_string(),
        ));
    }
    if !profile.backend.available {
        return Err(MorphError::Tool(format!(
            "selected backend {} is unavailable; run `morph doctor {}`",
            profile.backend.kind,
            root.display()
        )));
    }
    let config = MorphConfig::load(&profile.root)?;
    let executable = std::env::current_exe()?;
    let context = BackendContext {
        profile: profile.clone(),
        config,
        verbose,
        executable,
    };
    let backend = backend_for(profile.backend.kind);
    let mut session = backend.start(context)?;
    let mut watcher = FileWatcher::new(session.watch_paths(), Duration::from_millis(250))?;
    println!("[morph] development session started; waiting for changes (Ctrl-C to stop)");
    loop {
        if !session.is_alive()? {
            session.stop();
            return Err(MorphError::Child(
                "development host exited before Morph stopped it".to_string(),
            ));
        }
        let changed = watcher.wait_for_change()?;
        let report = session.reload(&changed)?;
        println!(
            "[morph] reload {}: {} (generation {}, state_preserved={})",
            report.status, report.message, report.generation, report.preserved_state
        );
        if report.status == ReloadStatus::RestartRequired {
            println!("[morph] restart required; no automatic restart was performed");
        }
    }
}

fn backend_for(kind: BackendKind) -> Box<dyn Backend> {
    match kind {
        BackendKind::CNative => Box::new(CBackend),
        BackendKind::PythonControlled => Box::new(PythonBackend),
        BackendKind::DetectionOnly => Box::new(UnsupportedBackend),
    }
}

struct UnsupportedBackend;

impl Backend for UnsupportedBackend {
    fn id(&self) -> &'static str {
        "detection-only"
    }

    fn start(&self, _context: BackendContext) -> MorphResult<Box<dyn BackendSession>> {
        Err(MorphError::Unsupported(
            "no reload backend is available".to_string(),
        ))
    }
}

fn parse_path_and_verbose(args: &[String]) -> MorphResult<(PathBuf, bool)> {
    let mut root = None;
    let mut verbose = false;
    for arg in args {
        match arg.as_str() {
            "--verbose" | "-v" => verbose = true,
            value if value.starts_with('-') => {
                return Err(MorphError::InvalidArguments(format!(
                    "unknown option {value}"
                )))
            }
            value if root.is_none() => root = Some(PathBuf::from(value)),
            value => {
                return Err(MorphError::InvalidArguments(format!(
                    "unexpected path argument {value:?}"
                )))
            }
        }
    }
    Ok((root.unwrap_or_else(|| PathBuf::from(".")), verbose))
}

fn required_tools(profile: &ProjectProfile) -> Vec<&'static str> {
    let mut tools = Vec::new();
    if profile.has_language(Language::C) || profile.has_language(Language::Cpp) {
        tools.extend(["cc"]);
    }
    if profile.has_language(Language::Rust) {
        tools.extend(["cargo"]);
    }
    if profile.has_language(Language::Python) {
        tools.extend(["python3"]);
    }
    if profile.has_language(Language::JavaScript) || profile.has_language(Language::TypeScript) {
        tools.extend(["node"]);
    }
    if profile.has_language(Language::Go) {
        tools.extend(["go"]);
    }
    tools
}

fn print_profile(profile: &ProjectProfile, verbose: bool) {
    println!("Project profile");
    println!("  root:       {}", profile.root.display());
    println!(
        "  confidence: {}%{}",
        profile.confidence,
        if profile.ambiguous {
            " (ambiguous)"
        } else {
            ""
        }
    );
    println!("  languages:  {}", display_languages(&profile.languages));
    println!(
        "  build:      {}",
        display_build_systems(&profile.build_systems)
    );
    println!("  frameworks: {}", display_or_none(&profile.frameworks));
    println!("  runtimes:   {}", display_or_none(&profile.runtimes));
    println!(
        "  backend:    {} ({})",
        profile.backend.name, profile.backend.kind
    );
    println!("  mode:       {}", profile.backend.mode);
    println!("  available:  {}", profile.backend.available);
    println!("  capabilities:");
    for capability in &profile.backend.capabilities {
        println!("    - {capability}");
    }
    let limitations = profile
        .limitations
        .iter()
        .chain(profile.backend.limitations.iter())
        .collect::<Vec<_>>();
    if !limitations.is_empty() {
        println!("  limitations:");
        for limitation in limitations {
            println!("    - {limitation}");
        }
    }
    if !profile.manifests.is_empty() {
        println!(
            "  manifests:  {}",
            display_paths(&profile.root, &profile.manifests)
        );
    }
    if verbose {
        println!("  project roots:");
        for root in &profile.project_roots {
            println!("    - {}", root.display());
        }
        if !profile.backend.missing_tools.is_empty() {
            println!(
                "  missing tools: {}",
                profile.backend.missing_tools.join(", ")
            );
        }
    }
}

fn print_backends() {
    println!("Morph backends");
    println!("  c-native");
    println!("    mode: native shared-library generation");
    println!("    applies: C projects with an explicit Morph ABI host boundary");
    println!("    state: host-owned state survives; old libraries remain mapped");
    println!("    limits: does not reload arbitrary executables or unsafe layouts");
    println!("  python-controlled");
    println!("    mode: controlled in-process module swap");
    println!("    applies: Python apps using morph_runtime.Runtime and module slots");
    println!("    state: Runtime-owned state survives; versioned migration is optional");
    println!("    limits: stale from-import references and arbitrary side effects are unsupported");
    println!("  detection-only");
    println!("    applies: Rust, JavaScript/TypeScript, Go, C++, and unconfigured projects");
    println!("    state: no reload claim; reports what integration is missing");
}

fn print_help() {
    println!(
        "Morph — capability-aware hot reload\n\n\
Usage:\n  morph init [PATH] [--force]\n  morph dev [PATH] [--verbose]\n  morph detect [PATH] [--verbose]\n  morph doctor [PATH] [--verbose]\n  morph backends\n  morph demo [c|python|all]\n\n\
Morph only activates changes through an explicit, tested runtime boundary.\n\
Use `morph detect .` before `morph dev .` when a project is unfamiliar."
    );
}

fn display_languages(languages: &[Language]) -> String {
    display_or_none(
        &languages
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}

fn display_build_systems(build_systems: &[BuildSystem]) -> String {
    display_or_none(
        &build_systems
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
    )
}

fn display_or_none(values: &[String]) -> String {
    if values.is_empty() {
        "none".to_string()
    } else {
        values.join(", ")
    }
}

fn display_paths(root: &Path, paths: &[PathBuf]) -> String {
    paths
        .iter()
        .map(|path| {
            path.strip_prefix(root)
                .unwrap_or(path)
                .display()
                .to_string()
        })
        .collect::<Vec<_>>()
        .join(", ")
}

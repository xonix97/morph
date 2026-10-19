use std::fmt::{Display, Formatter};
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    C,
    Cpp,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    Go,
}

impl Display for Language {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::C => "C",
            Self::Cpp => "C++",
            Self::Rust => "Rust",
            Self::Python => "Python",
            Self::JavaScript => "JavaScript",
            Self::TypeScript => "TypeScript",
            Self::Go => "Go",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BuildSystem {
    CMake,
    Make,
    Cargo,
    PyProject,
    PipRequirements,
    Npm,
    GoModules,
    Unknown,
}

impl Display for BuildSystem {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::CMake => "CMake",
            Self::Make => "Make",
            Self::Cargo => "Cargo",
            Self::PyProject => "pyproject.toml",
            Self::PipRequirements => "requirements.txt",
            Self::Npm => "npm/package.json",
            Self::GoModules => "Go modules",
            Self::Unknown => "unknown",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    CNative,
    PythonControlled,
    DetectionOnly,
}

impl Display for BackendKind {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::CNative => "c-native",
            Self::PythonControlled => "python-controlled",
            Self::DetectionOnly => "detection-only",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadMode {
    InProcessModuleSwap,
    NativeLibraryGeneration,
    ProcessRestart,
    DetectionOnly,
}

impl Display for ReloadMode {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::InProcessModuleSwap => "controlled in-process module swap",
            Self::NativeLibraryGeneration => "native shared-library generation",
            Self::ProcessRestart => "supervised process restart",
            Self::DetectionOnly => "detection only",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct BackendInfo {
    pub kind: BackendKind,
    pub name: String,
    pub mode: ReloadMode,
    pub available: bool,
    pub capabilities: Vec<String>,
    pub limitations: Vec<String>,
    pub missing_tools: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectProfile {
    pub root: PathBuf,
    pub project_roots: Vec<PathBuf>,
    pub languages: Vec<Language>,
    pub build_systems: Vec<BuildSystem>,
    pub manifests: Vec<PathBuf>,
    pub frameworks: Vec<String>,
    pub runtimes: Vec<String>,
    pub backend: BackendInfo,
    pub confidence: u8,
    pub ambiguous: bool,
    pub limitations: Vec<String>,
}

impl ProjectProfile {
    pub fn has_language(&self, language: Language) -> bool {
        self.languages.contains(&language)
    }
}

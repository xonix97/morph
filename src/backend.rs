//! Stable backend contract used by Morph's orchestrator.
//!
//! A backend owns language/runtime-specific work. The runtime only knows how
//! to start a session, ask what files it watches, forward debounced changes,
//! and surface an explicit outcome. A backend must never report `Applied` when
//! it merely restarted a process or only detected a file change.

use crate::config::MorphConfig;
use crate::error::MorphResult;
use crate::model::ProjectProfile;
use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReloadStatus {
    Applied,
    Rejected,
    RestartRequired,
    BuildFailed,
    Unsupported,
    Indeterminate,
}

impl std::fmt::Display for ReloadStatus {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let value = match self {
            Self::Applied => "applied",
            Self::Rejected => "rejected",
            Self::RestartRequired => "restart-required",
            Self::BuildFailed => "build-failed",
            Self::Unsupported => "unsupported",
            Self::Indeterminate => "indeterminate",
        };
        formatter.write_str(value)
    }
}

#[derive(Debug, Clone)]
pub struct ReloadReport {
    pub status: ReloadStatus,
    pub message: String,
    pub preserved_state: bool,
    pub generation: u64,
}

#[derive(Debug, Clone)]
pub struct BackendContext {
    pub profile: ProjectProfile,
    pub config: MorphConfig,
    pub verbose: bool,
    pub executable: PathBuf,
}

pub trait Backend {
    fn id(&self) -> &'static str;
    fn start(&self, context: BackendContext) -> MorphResult<Box<dyn BackendSession>>;
}

pub trait BackendSession {
    fn watch_paths(&self) -> Vec<PathBuf>;
    fn reload(&mut self, changed: &[PathBuf]) -> MorphResult<ReloadReport>;
    fn is_alive(&mut self) -> MorphResult<bool>;
    fn stop(&mut self);
}

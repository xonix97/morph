//! Morph's backend-neutral orchestration library.
//!
//! The binary is intentionally dependency-free. The first supported runtime
//! integrations are a versioned C shared-library boundary and a controlled
//! Python module boundary. Other languages are detected and reported without
//! being misrepresented as reloadable.

pub mod backend;
pub mod c_backend;
pub mod c_host;
pub mod config;
pub mod demo;
pub mod detect;
pub mod error;
pub mod file_ops;
pub mod init;
pub mod model;
pub mod python_backend;
pub mod runtime;
pub mod watch;

pub use error::{MorphError, MorphResult};
pub use model::{BackendKind, ProjectProfile};

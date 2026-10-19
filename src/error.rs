use std::fmt::{Display, Formatter};

pub type MorphResult<T> = Result<T, MorphError>;

#[derive(Debug)]
pub enum MorphError {
    InvalidArguments(String),
    Io(std::io::Error),
    Config(String),
    Detection(String),
    Tool(String),
    Build(String),
    Backend(String),
    Unsupported(String),
    Child(String),
}

impl MorphError {
    pub fn exit_code(&self) -> i32 {
        match self {
            Self::InvalidArguments(_) => 2,
            Self::Unsupported(_) => 3,
            Self::Tool(_) => 4,
            Self::Build(_) => 5,
            Self::Child(_) => 6,
            _ => 1,
        }
    }
}

impl Display for MorphError {
    fn fmt(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidArguments(message) => write!(formatter, "invalid arguments: {message}"),
            Self::Io(error) => write!(formatter, "{error}"),
            Self::Config(message) => write!(formatter, "configuration: {message}"),
            Self::Detection(message) => write!(formatter, "detection: {message}"),
            Self::Tool(message) => write!(formatter, "toolchain: {message}"),
            Self::Build(message) => write!(formatter, "build: {message}"),
            Self::Backend(message) => write!(formatter, "backend: {message}"),
            Self::Unsupported(message) => write!(formatter, "unsupported: {message}"),
            Self::Child(message) => write!(formatter, "child process: {message}"),
        }
    }
}

impl std::error::Error for MorphError {}

impl From<std::io::Error> for MorphError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

//! Error types and process exit codes.

use std::fmt;

#[derive(Debug)]
pub enum CleanOsError {
    Usage(String),
    ProbeFatal(String),
    Io(String),
}

impl fmt::Display for CleanOsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CleanOsError::Usage(m) | CleanOsError::ProbeFatal(m) | CleanOsError::Io(m) => {
                write!(f, "{m}")
            }
        }
    }
}

impl std::error::Error for CleanOsError {}

impl CleanOsError {
    pub fn exit_code(&self) -> u8 {
        match self {
            CleanOsError::Usage(_) => 2,
            CleanOsError::ProbeFatal(_) | CleanOsError::Io(_) => 1,
        }
    }
}

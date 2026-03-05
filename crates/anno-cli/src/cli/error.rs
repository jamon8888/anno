use std::borrow::Cow;

use thiserror::Error;

use super::exit_codes;

/// CLI error type with structured error codes.
#[derive(Debug, Error)]
pub enum CliError {
    /// Error with a message and exit code.
    #[error("{message}")]
    Message {
        /// Exit code to return.
        code: u8,
        /// Error message.
        message: Cow<'static, str>,
    },
}

impl CliError {
    /// Get the exit code for this error.
    pub fn exit_code(&self) -> u8 {
        match self {
            Self::Message { code, .. } => *code,
        }
    }
}

impl From<String> for CliError {
    fn from(message: String) -> Self {
        Self::Message {
            code: exit_codes::ERROR_GENERAL,
            message: Cow::Owned(message),
        }
    }
}

impl From<&'static str> for CliError {
    fn from(message: &'static str) -> Self {
        Self::Message {
            code: exit_codes::ERROR_GENERAL,
            message: Cow::Borrowed(message),
        }
    }
}

impl From<serde_json::Error> for CliError {
    fn from(err: serde_json::Error) -> Self {
        Self::Message {
            code: exit_codes::ERROR_GENERAL,
            message: Cow::Owned(format!("JSON error: {}", err)),
        }
    }
}

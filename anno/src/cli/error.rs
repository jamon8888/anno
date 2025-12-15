use std::borrow::Cow;

use thiserror::Error;

use super::exit_codes;

#[derive(Debug, Error)]
pub enum CliError {
    #[error("{message}")]
    Message {
        code: u8,
        message: Cow<'static, str>,
    },
}

impl CliError {
    pub fn args(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Message {
            code: exit_codes::ERROR_ARGS,
            message: message.into(),
        }
    }

    pub fn input_not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Message {
            code: exit_codes::ERROR_INPUT_NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn model_not_found(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Message {
            code: exit_codes::ERROR_MODEL_NOT_FOUND,
            message: message.into(),
        }
    }

    pub fn timeout(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Message {
            code: exit_codes::ERROR_TIMEOUT,
            message: message.into(),
        }
    }

    pub fn oom(message: impl Into<Cow<'static, str>>) -> Self {
        Self::Message {
            code: exit_codes::ERROR_OOM,
            message: message.into(),
        }
    }

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

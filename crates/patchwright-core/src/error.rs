use std::error::Error;
use std::fmt::{Display, Formatter};

pub type Result<T> = std::result::Result<T, PatchwrightError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatchwrightError {
    InvalidInput(String),
    Io(String),
    CommandFailed(String),
    PolicyDenied(String),
    Verification(String),
    Model(String),
}

impl Display for PatchwrightError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidInput(message)
            | Self::Io(message)
            | Self::CommandFailed(message)
            | Self::PolicyDenied(message)
            | Self::Verification(message)
            | Self::Model(message) => f.write_str(message),
        }
    }
}

impl Error for PatchwrightError {}

impl From<std::io::Error> for PatchwrightError {
    fn from(value: std::io::Error) -> Self {
        Self::Io(value.to_string())
    }
}

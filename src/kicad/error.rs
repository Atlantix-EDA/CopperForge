use std::fmt;

#[derive(Debug)]
pub enum KicadError {
    IoError(std::io::Error),
    ParseError(String),
    InvalidFormat(String),
    MissingField(String),
    UnexpectedToken(String),
}

impl fmt::Display for KicadError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            KicadError::IoError(e) => write!(f, "IO error: {}", e),
            KicadError::ParseError(msg) => write!(f, "Parse error: {}", msg),
            KicadError::InvalidFormat(msg) => write!(f, "Invalid format: {}", msg),
            KicadError::MissingField(field) => write!(f, "Missing field: {}", field),
            KicadError::UnexpectedToken(token) => write!(f, "Unexpected token: {}", token),
        }
    }
}

impl std::error::Error for KicadError {}

impl From<std::io::Error> for KicadError {
    fn from(error: std::io::Error) -> Self {
        KicadError::IoError(error)
    }
}

pub type Result<T> = std::result::Result<T, KicadError>;
use std::fmt;

#[derive(Debug)]
pub struct AdbError {
    pub message: String,
}

impl AdbError {
    pub fn new(msg: impl Into<String>) -> Self {
        Self {
            message: msg.into(),
        }
    }
}

impl fmt::Display for AdbError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl From<std::io::Error> for AdbError {
    fn from(e: std::io::Error) -> Self {
        AdbError::new(e.to_string())
    }
}

impl From<String> for AdbError {
    fn from(s: String) -> Self {
        AdbError::new(s)
    }
}

impl serde::Serialize for AdbError {
    fn serialize<S: serde::Serializer>(
        &self,
        serializer: S,
    ) -> std::result::Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.message)
    }
}

pub type AdbResult<T> = std::result::Result<T, AdbError>;

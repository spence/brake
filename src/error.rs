use std::fmt;

#[derive(Debug)]
pub enum Error {
  Os(String),
  ThreadGone,
}

impl fmt::Display for Error {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Error::Os(msg) => write!(f, "OS error: {}", msg),
      Error::ThreadGone => write!(f, "thread no longer exists"),
    }
  }
}

impl std::error::Error for Error {}

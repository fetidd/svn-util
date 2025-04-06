#[derive(thiserror::Error, Debug)]
pub enum Error {
    PathNotUnderVersionControl(String),
    BranchParseFailure,
    UnrecognisedStatus(String),
    Unknown(String),
    Io(#[from] std::io::Error),
}

impl PartialEq for Error {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Error::PathNotUnderVersionControl(p), Error::PathNotUnderVersionControl(p2)) => {
                p == p2
            }
            (Error::Io(error), Error::Io(other)) => error.kind() == other.kind(),
            (Error::Unknown(s), Error::Unknown(s2)) => s == s2,
            (Error::BranchParseFailure, Error::BranchParseFailure) => true,
            _ => false,
        }
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let msg = match self {
            Error::PathNotUnderVersionControl(p) => format!("Not svn controlled: {p}"),
            Error::Io(error) => error.to_string(),
            Error::Unknown(s) => s.clone(),
            Error::UnrecognisedStatus(status) => format!("Unrecognised status: {status}"),
            Error::BranchParseFailure => "failed to parse URL from svn info".into(),
        };
        write!(f, "{msg}")
    }
}

impl From<&str> for Error {
    fn from(value: &str) -> Self {
        match value {
            _ => Error::Unknown(value.into()), // consider any errors that fall back to Unknown to see iof they could have their own discriminant
        }
    }
}

impl From<&String> for Error {
    fn from(value: &String) -> Self {
        Error::from(value.as_str())
    }
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Error::from(value.as_str())
    }
}

pub type Result<T> = std::result::Result<T, Error>;

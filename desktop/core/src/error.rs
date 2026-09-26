//! The one error value the reviews API returns.
//!
//! `error` is what callers match on: `not_found`, `not_pending` when the
//! review is decided, withdrawn or expired, and `invalid` with violations
//! saying where. A violation's path is a JSON pointer into the offending
//! document, the shape plugins render. The statuses these answer with over
//! HTTP are the API's to decide; see `api::error`.

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Violation {
    pub path: String,
    pub message: String,
}

impl Violation {
    pub fn new(path: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            path: path.into(),
            message: message.into(),
        }
    }
}

#[derive(Debug)]
pub enum Error {
    /// What was not found, as the message names it: `review r_1`, `plugin hello`
    NotFound(String),
    NotPending(String),
    Invalid(Vec<Violation>),
    Internal(String),
}

impl Error {
    pub fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Invalid(vec![Violation::new(path, message)])
    }

    pub fn message(&self) -> String {
        match self {
            Error::NotFound(what) => format!("{what} not found"),
            Error::NotPending(id) => format!("review {id} is no longer pending"),
            Error::Invalid(violations) => {
                let mut lines = vec!["validation failed:".to_string()];
                for v in violations {
                    let path = if v.path.is_empty() { "/" } else { &v.path };
                    lines.push(format!("  {path}: {}", v.message));
                }
                lines.join("\n")
            }
            Error::Internal(message) => message.clone(),
        }
    }

    pub fn to_json(&self) -> Value {
        let (kind, violations) = match self {
            Error::NotFound(_) => ("not_found", Vec::new()),
            Error::NotPending(_) => ("not_pending", Vec::new()),
            Error::Invalid(v) => ("invalid", v.clone()),
            Error::Internal(_) => ("internal", Vec::new()),
        };
        json!({ "error": kind, "message": self.message(), "violations": violations })
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Error {}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Internal(format!("database: {error}"))
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Internal(format!("io: {error}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_keep_their_wording() {
        let e = Error::Invalid(vec![
            Violation::new("", "property 'undecided' is required"),
            Violation::new("/title", "is required"),
        ]);
        assert_eq!(
            e.message(),
            "validation failed:\n  /: property 'undecided' is required\n  /title: is required"
        );
        assert_eq!(e.to_json()["error"], "invalid");
        assert_eq!(
            Error::NotFound("review r_1".into()).to_json()["message"],
            "review r_1 not found"
        );
        assert_eq!(
            Error::NotPending("r_1".into()).to_json()["error"],
            "not_pending"
        );
    }
}

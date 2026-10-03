//! The one error value the reviews API returns.
//!
//! `error` is what callers match on: `not_found`, `not_pending` when the
//! review is decided, withdrawn or expired, `conflict` when it moved to
//! another version of its plugin meanwhile, `invalid` with violations
//! saying where, and `unavailable` when a source the app fetches from
//! could not be reached. A violation's path is a JSON pointer into the offending
//! document, the shape plugins render. The statuses these answer with over
//! HTTP are the API's to decide; see `api::error`.

use serde_json::{Value, json};

pub use pinrail_format::Violation;

#[derive(Debug)]
pub enum Error {
    /// What was not found, as the message names it: `review r_1`, `plugin hello`
    NotFound(String),
    NotPending(String),
    /// The review moved to another version of its plugin than the one the
    /// request was made against, as the message says
    Conflict(String),
    Invalid(Vec<Violation>),
    Internal(String),
    /// The database failed, with the error it gave
    Database(rusqlite::Error),
    /// Reading or writing a file failed, with the error it gave
    Io(std::io::Error),
    /// Another Pinrail holds the data directory, as the message says
    InUse(String),
    /// A source the app fetches from, such as a repository or GitHub,
    /// could not be reached or failed on its side: not the caller's
    /// mistake, and worth trying again later
    Unavailable(String),
}

impl Error {
    pub fn invalid(path: impl Into<String>, message: impl Into<String>) -> Self {
        Error::Invalid(vec![Violation::new(path, message)])
    }

    pub fn message(&self) -> String {
        match self {
            Error::NotFound(what) => format!("{what} not found"),
            Error::NotPending(id) => format!("review {id} is no longer pending"),
            Error::Conflict(message) => message.clone(),
            Error::Invalid(violations) => {
                let mut lines = vec!["validation failed:".to_string()];
                for v in violations {
                    let path = if v.path.is_empty() { "/" } else { &v.path };
                    lines.push(format!("  {path}: {}", v.message));
                }
                lines.join("\n")
            }
            Error::Internal(message) => message.clone(),
            Error::Database(error) => format!("database: {error}"),
            Error::Io(error) => format!("io: {error}"),
            Error::InUse(message) => message.clone(),
            Error::Unavailable(message) => message.clone(),
        }
    }

    pub fn to_json(&self) -> Value {
        let (kind, violations) = match self {
            Error::NotFound(_) => ("not_found", Vec::new()),
            Error::NotPending(_) => ("not_pending", Vec::new()),
            Error::Conflict(_) => ("conflict", Vec::new()),
            Error::Invalid(v) => ("invalid", v.clone()),
            Error::Internal(_) | Error::Database(_) | Error::Io(_) => ("internal", Vec::new()),
            Error::InUse(_) => ("in_use", Vec::new()),
            Error::Unavailable(_) => ("unavailable", Vec::new()),
        };
        json!({ "error": kind, "message": self.message(), "violations": violations })
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message())
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Database(error) => Some(error),
            Error::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Database(error)
    }
}

impl From<std::io::Error> for Error {
    fn from(error: std::io::Error) -> Self {
        Error::Io(error)
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

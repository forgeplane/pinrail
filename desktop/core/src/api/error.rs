//! An application error as an HTTP answer: the status each kind is given,
//! and the body callers match on.
//!
//! The error itself says what went wrong, not what a server should reply. The
//! mapping lives here, in the only layer that has a status code to give: a
//! review that is not there is `404`, one that has already ended is `409`, a
//! document that does not fit its schema is `422`, and anything we did wrong
//! is `500` and goes to the log.

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

use crate::error::Error;

/// An [`Error`] on its way out over HTTP. Handlers return this, and `?`
/// converts for them.
#[derive(Debug)]
pub struct ApiError(pub Error);

impl From<Error> for ApiError {
    fn from(error: Error) -> Self {
        ApiError(error)
    }
}

/// The status each kind of error answers with.
fn status(error: &Error) -> StatusCode {
    match error {
        Error::NotFound(_) => StatusCode::NOT_FOUND,
        Error::NotPending(_) => StatusCode::CONFLICT,
        Error::Invalid(_) => StatusCode::UNPROCESSABLE_ENTITY,
        Error::Internal(_) | Error::Database(_) | Error::Io(_) => StatusCode::INTERNAL_SERVER_ERROR,
        Error::InUse(_) => StatusCode::CONFLICT,
        Error::Unavailable(_) => StatusCode::BAD_GATEWAY,
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        if status(&self.0) == StatusCode::INTERNAL_SERVER_ERROR {
            eprintln!("pinrail: {}", self.0);
        }
        (status(&self.0), Json(self.0.to_json())).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::Violation;

    #[test]
    fn each_kind_of_error_has_its_status() {
        assert_eq!(
            status(&Error::NotFound("review r_1".into())),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            status(&Error::NotPending("r_1".into())),
            StatusCode::CONFLICT
        );
        assert_eq!(
            status(&Error::Invalid(vec![Violation::new(
                "/title",
                "is required"
            )])),
            StatusCode::UNPROCESSABLE_ENTITY
        );
        assert_eq!(
            status(&Error::Internal("disk on fire".into())),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(&Error::Io(std::io::Error::other("disk on fire"))),
            StatusCode::INTERNAL_SERVER_ERROR
        );
        assert_eq!(
            status(&Error::Database(rusqlite::Error::InvalidQuery)),
            StatusCode::INTERNAL_SERVER_ERROR
        );
    }

    #[test]
    fn the_body_carries_the_kind_the_message_and_the_violations() {
        let error = ApiError(Error::Invalid(vec![Violation::new(
            "/title",
            "is required",
        )]));
        let body = error.0.to_json();
        assert_eq!(body["error"], "invalid");
        assert_eq!(body["violations"][0]["path"], "/title");
    }
}

//! Shared database helpers used by the route handlers.

use axum::http::StatusCode;
use sqlx::Error;

/// Maps a unique-constraint violation (SQLSTATE 23505) to 409 Conflict;
/// anything else is logged and becomes 500.
pub fn conflict_or_500(e: Error) -> StatusCode {
    if let Error::Database(db_err) = &e {
        if db_err.code().as_deref() == Some("23505") {
            return StatusCode::CONFLICT;
        }
    }
    tracing::error!(?e, "database error");
    StatusCode::INTERNAL_SERVER_ERROR
}

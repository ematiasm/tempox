//! `GET /api/day` — which instants are "today" for this shop (S7).
//!
//! This is the one place the locale reaches the server, and it does so as
//! *configuration*, not as translation: the answer is four instants, not a
//! formatted date. The zone comes from `SHOP_TIMEZONE` and never from a request
//! parameter — a client that could name its own timezone could file a sale under
//! a day the shop never worked.

use axum::{
    Json, Router,
    extract::{Query, State},
    http::StatusCode,
    routing::get,
};
use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::AppState;
use crate::day::{self, DayRange};

pub fn routes() -> Router<AppState> {
    Router::new().route("/day", get(get_day))
}

#[derive(Debug, Deserialize)]
pub struct DayQuery {
    /// The instant to scope. Absent means now, which is what the ledger asks for
    /// when it opens on the current day.
    pub at: Option<String>,
}

async fn get_day(
    State(state): State<AppState>,
    Query(params): Query<DayQuery>,
) -> Result<Json<DayRange>, StatusCode> {
    // A malformed instant is a bad request, not a 500 and not a fallback: the
    // ledger would rather show its read error than silently scope itself to a
    // day it made up (S9).
    let at: DateTime<Utc> = match params.at.as_deref() {
        Some(raw) => day::parse_instant(raw).ok_or(StatusCode::BAD_REQUEST)?,
        None => Utc::now(),
    };
    Ok(Json(day::day_range(state.shop_timezone, at)))
}

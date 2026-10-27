use axum::{Json, Router, extract::State, http::StatusCode, routing::get};
use serde_json::{Value, json};

use crate::AppState;

pub fn routes() -> Router<AppState> {
    Router::new().route("/health", get(health))
}

/// Whether the process can serve, answered without touching the database.
///
/// **The defect this replaces was a check that always said `ok`.** `/health`
/// answered `{"status":"ok"}` unconditionally, so during a stall it kept telling a
/// supervisor the shop was fine. Measured: with twelve requests hung over a
/// ten-connection pool, `/api/products` stopped answering while `/health` replied
/// `200` in 6 ms.
///
/// **The first fix was wrong in the opposite direction**, and review caught it. It
/// answered `503` when `num_idle() == 0 || size() == 0`, which confuses "nothing is
/// free right now" with "cannot serve":
///
/// | pool state | free slot? | serves? | first fix | correct |
/// |---|---|---|---|---|
/// | empty (lazy, or reaped while quiet) | yes | yes, opens one | `503` | `200` |
/// | one checked out of ten | 9 | yes | `503` | `200` |
/// | all ten checked out | none | waits | `503` | `503` |
///
/// The two wrong rows are the common case, not an edge: an empty pool is a lazy
/// pool before its first request, and a pool whose idle connections a database
/// restart dropped. A supervisor acting on that `503` removes a backend that would
/// have served the next request immediately — the check causing the outage it
/// reports.
///
/// **So the predicate is saturation, not emptiness.** `size()` counts the
/// connections the pool holds, checked out or idle; `num_idle()` the ones free. The
/// pool cannot serve only when it is at its maximum *and* nothing is idle, because
/// then a request must wait for someone to give a connection back. `min_connections`
/// is left at its default of zero, which is exactly why an empty pool must be
/// allowed to be healthy.
///
/// **No SQL, and no `acquire()`.** A `SELECT 1` here would queue behind the
/// exhausted pool it exists to report, and once it timed out it would report the
/// pool as broken rather than busy. Both counters are in-memory reads, so the check
/// stays fast in the state that matters.
///
/// What this deliberately does **not** claim: it says nothing about the database
/// being reachable. A pool that cannot connect reports `ok` until its first request
/// fails. A liveness check that also probed the database would have to block on
/// exactly the resource under suspicion, which is the trap above; reachability is
/// the business of the requests themselves, and `docs/en/architecture.md` keeps
/// real observability out of scope for this starter.
async fn health(State(state): State<AppState>) -> (StatusCode, Json<Value>) {
    let size = state.db.size();
    let idle = state.db.num_idle();
    let max = state.db.options().get_max_connections();

    // Saturated, and only then. `size >= max` rather than `== max`: a pool must not
    // report healthy because a counter went past a value it should not reach.
    let saturated = idle == 0 && size >= max;

    if saturated {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Json(json!({ "status": "unavailable" })),
        )
    } else {
        (StatusCode::OK, Json(json!({ "status": "ok" })))
    }
}

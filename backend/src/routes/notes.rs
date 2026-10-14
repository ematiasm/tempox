use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{delete, get},
};
use uuid::Uuid;

use crate::AppState;
use crate::models::note::{NewNote, Note};

pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/notes", get(list_notes).post(create_note))
        .route("/notes/{id}", delete(delete_note))
}

async fn list_notes(State(state): State<AppState>) -> Result<Json<Vec<Note>>, StatusCode> {
    let rows = sqlx::query_as!(
        Note,
        "SELECT id, text, created_at FROM notes ORDER BY created_at DESC LIMIT 100"
    )
    .fetch_all(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "list_notes failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok(Json(rows))
}

async fn create_note(
    State(state): State<AppState>,
    Json(body): Json<NewNote>,
) -> Result<(StatusCode, Json<Note>), StatusCode> {
    let text = body.text.trim();
    if text.is_empty() || text.len() > 500 {
        return Err(StatusCode::BAD_REQUEST);
    }
    let row = sqlx::query_as!(
        Note,
        "INSERT INTO notes (text) VALUES ($1) RETURNING id, text, created_at",
        text
    )
    .fetch_one(&state.db)
    .await
    .map_err(|e| {
        tracing::error!(?e, "create_note failed");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    Ok((StatusCode::CREATED, Json(row)))
}

async fn delete_note(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, StatusCode> {
    let res = sqlx::query!("DELETE FROM notes WHERE id = $1", id)
        .execute(&state.db)
        .await
        .map_err(|e| {
            tracing::error!(?e, "delete_note failed");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;
    if res.rows_affected() == 0 {
        Err(StatusCode::NOT_FOUND)
    } else {
        Ok(StatusCode::NO_CONTENT)
    }
}

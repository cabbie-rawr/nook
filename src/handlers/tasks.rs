use askama::Template;
use axum::extract::{Path, State};
use axum::response::{Html, IntoResponse, Redirect};
use axum::{http::StatusCode, Form};
use chrono::{DateTime, NaiveDate, Utc};
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::models::{Task, TaskPriority, TaskStatus};
use crate::state::AppState;
use crate::templates::TaskRowTemplate;
use crate::view::{task_status_options, TaskView};

#[derive(Deserialize)]
pub struct CreateTaskForm {
    pub title: String,
    #[serde(default)]
    pub notes: String,
    pub priority: TaskPriority,
    #[serde(default)]
    pub due_at: String,
}

/// Accepts a plain `YYYY-MM-DD` (from the date-only `<input type="date">` UI,
/// defaulted to end-of-day) or a `YYYY-MM-DDTHH:MM` local datetime (from the
/// quick-add natural-language parser, which knows the actual time).
fn parse_due_at(raw: &str) -> Result<Option<DateTime<Utc>>, AppError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Ok(None);
    }
    if let Ok(naive) = chrono::NaiveDateTime::parse_from_str(trimmed, "%Y-%m-%dT%H:%M") {
        return Ok(Some(naive.and_utc()));
    }
    let date = NaiveDate::parse_from_str(trimmed, "%Y-%m-%d")
        .map_err(|_| AppError::BadRequest("Invalid due date.".into()))?;
    let datetime = date
        .and_hms_opt(23, 59, 0)
        .ok_or_else(|| AppError::BadRequest("Invalid due date.".into()))?
        .and_utc();
    Ok(Some(datetime))
}

/// Confirms `task_id` belongs (through its space) to `user_id`.
async fn assert_owns_task(pool: &SqlitePool, task_id: i64, user_id: i64) -> Result<(), AppError> {
    let found = sqlx::query_scalar::<_, i64>(
        "SELECT tasks.id FROM tasks JOIN spaces ON spaces.id = tasks.space_id
         WHERE tasks.id = ? AND spaces.user_id = ?",
    )
    .bind(task_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?;
    found.map(|_| ()).ok_or(AppError::NotFound)
}

pub async fn create_task(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(space_id): Path<i64>,
    Form(form): Form<CreateTaskForm>,
) -> Result<impl IntoResponse, AppError> {
    let owns_space = sqlx::query_scalar::<_, i64>("SELECT id FROM spaces WHERE id = ? AND user_id = ?")
        .bind(space_id)
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?;
    if owns_space.is_none() {
        return Err(AppError::NotFound);
    }

    let title = form.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::BadRequest("Task title can't be empty.".into()));
    }
    let due_at = parse_due_at(&form.due_at)?;

    sqlx::query("INSERT INTO tasks (space_id, title, notes, priority, due_at) VALUES (?, ?, ?, ?, ?)")
        .bind(space_id)
        .bind(title)
        .bind(form.notes.trim().to_string())
        .bind(form.priority)
        .bind(due_at)
        .execute(&state.pool)
        .await?;

    Ok(Redirect::to(&format!("/spaces/{space_id}")))
}

#[derive(Deserialize)]
pub struct StatusForm {
    pub status: TaskStatus,
}

pub async fn update_status(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
    Form(form): Form<StatusForm>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;

    let completed_at = matches!(form.status, TaskStatus::Done).then(Utc::now);
    sqlx::query("UPDATE tasks SET status = ?, completed_at = ?, updated_at = ? WHERE id = ?")
        .bind(form.status)
        .bind(completed_at)
        .bind(Utc::now())
        .bind(task_id)
        .execute(&state.pool)
        .await?;

    let task = sqlx::query_as::<_, Task>("SELECT * FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&state.pool)
        .await?;

    let view = TaskView::from(task);
    let status_options = task_status_options(view.status_value);
    let html = TaskRowTemplate { task: view, status_options }.render().map_err(|e| anyhow::anyhow!(e))?;

    Ok(Html(html))
}

pub async fn delete_task(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;
    sqlx::query("DELETE FROM tasks WHERE id = ?")
        .bind(task_id)
        .execute(&state.pool)
        .await?;
    Ok(StatusCode::OK)
}

//! The task detail page, its plan steps, attachments, and the small mutation
//! endpoints (log focus minutes, optimistic complete-toggle) the Today page's
//! client-side JS calls directly.

use axum::body::Bytes;
use axum::extract::{Multipart, Path, State};
use axum::http::{header, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::SqlitePool;
use std::path::PathBuf;
use uuid::Uuid;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::models::{Attachment, PlanStep, Task, MAX_ATTACHMENT_BYTES};
use crate::state::AppState;
use crate::templates::TaskDetailTemplate;
use crate::view::TaskView;

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

pub async fn show(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let task = sqlx::query_as::<_, Task>(
        r#"SELECT tasks.* FROM tasks JOIN spaces ON spaces.id = tasks.space_id
           WHERE tasks.id = ? AND spaces.user_id = ?"#,
    )
    .bind(task_id)
    .bind(user.id)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(AppError::NotFound)?;

    let (space_name, space_color): (String, crate::models::SpaceColor) =
        sqlx::query_as("SELECT name, color FROM spaces WHERE id = ?")
            .bind(task.space_id)
            .fetch_one(&state.pool)
            .await?;

    sqlx::query("UPDATE tasks SET last_opened_at = ? WHERE id = ?")
        .bind(Utc::now())
        .bind(task_id)
        .execute(&state.pool)
        .await?;

    let steps = sqlx::query_as::<_, PlanStep>("SELECT * FROM plan_steps WHERE task_id = ? ORDER BY position ASC")
        .bind(task_id)
        .fetch_all(&state.pool)
        .await?;

    let attachments =
        sqlx::query_as::<_, Attachment>("SELECT * FROM attachments WHERE task_id = ? ORDER BY uploaded_at DESC")
            .bind(task_id)
            .fetch_all(&state.pool)
            .await?;

    Ok(TaskDetailTemplate {
        display_name: user.display_name,
        space_name,
        space_color: space_color.as_str(),
        task: TaskView::from(task),
        steps,
        attachments,
        max_attachment_mb: MAX_ATTACHMENT_BYTES / (1024 * 1024),
    })
}

#[derive(Deserialize)]
pub struct StepForm {
    pub text: String,
}

pub async fn create_step(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
    Form(form): Form<StepForm>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;
    let text = form.text.trim().to_string();
    if text.is_empty() {
        return Err(AppError::BadRequest("Step text can't be empty.".into()));
    }
    let next_position = sqlx::query_scalar::<_, i64>(
        "SELECT COALESCE(MAX(position), -1) + 1 FROM plan_steps WHERE task_id = ?",
    )
    .bind(task_id)
    .fetch_one(&state.pool)
    .await?;

    sqlx::query("INSERT INTO plan_steps (task_id, position, text) VALUES (?, ?, ?)")
        .bind(task_id)
        .bind(next_position)
        .bind(text)
        .execute(&state.pool)
        .await?;

    Ok(Redirect::to(&format!("/tasks/{task_id}")))
}

async fn assert_owns_step(pool: &SqlitePool, step_id: i64, user_id: i64) -> Result<i64, AppError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT plan_steps.task_id FROM plan_steps
         JOIN tasks ON tasks.id = plan_steps.task_id
         JOIN spaces ON spaces.id = tasks.space_id
         WHERE plan_steps.id = ? AND spaces.user_id = ?",
    )
    .bind(step_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn toggle_step(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(step_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_step(&state.pool, step_id, user.id).await?;
    sqlx::query("UPDATE plan_steps SET done = NOT done WHERE id = ?")
        .bind(step_id)
        .execute(&state.pool)
        .await?;
    let step = sqlx::query_as::<_, PlanStep>("SELECT * FROM plan_steps WHERE id = ?")
        .bind(step_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(Html(render_step(&step)))
}

pub async fn delete_step(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(step_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_step(&state.pool, step_id, user.id).await?;
    sqlx::query("DELETE FROM plan_steps WHERE id = ?").bind(step_id).execute(&state.pool).await?;
    Ok(StatusCode::OK)
}

fn escape_html(raw: &str) -> String {
    raw.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

#[derive(Serialize)]
pub struct PlanStepJson {
    pub text: String,
    pub done: bool,
}

/// Feeds the Focus session overlay's plan-step list without a full page load.
pub async fn plan_steps_json(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;
    let steps = sqlx::query_as::<_, PlanStep>("SELECT * FROM plan_steps WHERE task_id = ? ORDER BY position ASC")
        .bind(task_id)
        .fetch_all(&state.pool)
        .await?;
    let json: Vec<PlanStepJson> = steps.into_iter().map(|s| PlanStepJson { text: s.text, done: s.done }).collect();
    Ok(axum::Json(json))
}

pub fn render_step(step: &PlanStep) -> String {
    format!(
        r##"<li class="plan-step{done_class}" id="step-{id}">
  <label><input type="checkbox" {checked} hx-post="/steps/{id}/toggle" hx-target="#step-{id}" hx-swap="outerHTML"><span>{text}</span></label>
  <button class="link-button danger" hx-post="/steps/{id}/delete" hx-target="#step-{id}" hx-swap="outerHTML" hx-confirm="Remove this step?" type="button">Remove</button>
</li>"##,
        done_class = if step.done { " is-done" } else { "" },
        id = step.id,
        checked = if step.done { "checked" } else { "" },
        text = escape_html(&step.text),
    )
}

// ---------------------------------------------------------------------------
// Attachments
// ---------------------------------------------------------------------------

fn storage_dir() -> PathBuf {
    PathBuf::from("storage")
}

pub async fn upload_attachment(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;

    let mut saved = false;
    while let Some(field) = multipart.next_field().await.map_err(|e| anyhow::anyhow!(e))? {
        if field.name() != Some("file") {
            continue;
        }
        let original_filename = field.file_name().unwrap_or("upload").to_string();
        let mime_type = field.content_type().unwrap_or("application/octet-stream").to_string();
        let data: Bytes = field.bytes().await.map_err(|e| anyhow::anyhow!(e))?;

        if data.len() as i64 > MAX_ATTACHMENT_BYTES {
            return Err(AppError::BadRequest(format!(
                "File is too large (max {} MB).",
                MAX_ATTACHMENT_BYTES / (1024 * 1024)
            )));
        }
        if data.is_empty() {
            continue;
        }

        let stored_name = format!("{}-{}", Uuid::new_v4(), sanitize_filename(&original_filename));
        let dir = storage_dir();
        tokio::fs::create_dir_all(&dir).await.map_err(|e| anyhow::anyhow!(e))?;
        let full_path = dir.join(&stored_name);
        tokio::fs::write(&full_path, &data).await.map_err(|e| anyhow::anyhow!(e))?;

        sqlx::query(
            "INSERT INTO attachments (task_id, original_filename, stored_path, mime_type, size_bytes)
             VALUES (?, ?, ?, ?, ?)",
        )
        .bind(task_id)
        .bind(&original_filename)
        .bind(stored_name)
        .bind(&mime_type)
        .bind(data.len() as i64)
        .execute(&state.pool)
        .await?;
        saved = true;
    }

    if !saved {
        return Err(AppError::BadRequest("No file was uploaded.".into()));
    }

    Ok(Redirect::to(&format!("/tasks/{task_id}")))
}

fn sanitize_filename(name: &str) -> String {
    name.chars().map(|c| if c.is_alphanumeric() || c == '.' || c == '-' || c == '_' { c } else { '_' }).collect()
}

async fn owned_attachment(pool: &SqlitePool, attachment_id: i64, user_id: i64) -> Result<Attachment, AppError> {
    sqlx::query_as::<_, Attachment>(
        r#"SELECT attachments.* FROM attachments
           JOIN tasks ON tasks.id = attachments.task_id
           JOIN spaces ON spaces.id = tasks.space_id
           WHERE attachments.id = ? AND spaces.user_id = ?"#,
    )
    .bind(attachment_id)
    .bind(user_id)
    .fetch_optional(pool)
    .await?
    .ok_or(AppError::NotFound)
}

pub async fn download_attachment(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(attachment_id): Path<i64>,
) -> Result<Response, AppError> {
    let attachment = owned_attachment(&state.pool, attachment_id, user.id).await?;
    let bytes = tokio::fs::read(storage_dir().join(&attachment.stored_path))
        .await
        .map_err(|_| AppError::NotFound)?;

    Ok((
        [
            (header::CONTENT_TYPE, attachment.mime_type),
            (header::CONTENT_DISPOSITION, format!("attachment; filename=\"{}\"", attachment.original_filename)),
        ],
        bytes,
    )
        .into_response())
}

pub async fn delete_attachment(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(attachment_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let attachment = owned_attachment(&state.pool, attachment_id, user.id).await?;
    let _ = tokio::fs::remove_file(storage_dir().join(&attachment.stored_path)).await;
    sqlx::query("DELETE FROM attachments WHERE id = ?").bind(attachment_id).execute(&state.pool).await?;
    Ok(Redirect::to(&format!("/tasks/{}", attachment.task_id)))
}

// ---------------------------------------------------------------------------
// Focus minutes + optimistic complete-toggle
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct LogMinutesForm {
    pub minutes: i64,
}

#[derive(Serialize)]
pub struct LogMinutesResponse {
    pub logged_minutes: i64,
}

pub async fn log_minutes(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
    Form(form): Form<LogMinutesForm>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;
    let minutes = form.minutes.clamp(1, 24 * 60);
    sqlx::query("UPDATE tasks SET logged_minutes = logged_minutes + ?, updated_at = ? WHERE id = ?")
        .bind(minutes)
        .bind(Utc::now())
        .bind(task_id)
        .execute(&state.pool)
        .await?;
    let logged_minutes = sqlx::query_scalar::<_, i64>("SELECT logged_minutes FROM tasks WHERE id = ?")
        .bind(task_id)
        .fetch_one(&state.pool)
        .await?;
    Ok(axum::Json(LogMinutesResponse { logged_minutes }))
}

#[derive(Serialize)]
pub struct CompleteToggleResponse {
    pub id: i64,
    pub done: bool,
}

pub async fn complete_toggle(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(task_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    assert_owns_task(&state.pool, task_id, user.id).await?;
    let current_status =
        sqlx::query_scalar::<_, String>("SELECT status FROM tasks WHERE id = ?").bind(task_id).fetch_one(&state.pool).await?;

    let now_done = current_status != "done";
    let (new_status, completed_at) = if now_done { ("done", Some(Utc::now())) } else { ("todo", None) };

    sqlx::query("UPDATE tasks SET status = ?, completed_at = ?, updated_at = ? WHERE id = ?")
        .bind(new_status)
        .bind(completed_at)
        .bind(Utc::now())
        .bind(task_id)
        .execute(&state.pool)
        .await?;

    Ok(axum::Json(CompleteToggleResponse { id: task_id, done: now_done }))
}

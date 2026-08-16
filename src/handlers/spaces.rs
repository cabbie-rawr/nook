use askama::Template;
use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect};
use axum::Form;
use serde::Deserialize;
use sqlx::SqlitePool;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::models::{Space, SpaceColor, Task};
use crate::state::AppState;
use crate::templates::{DashboardTemplate, SpaceDetailTemplate, TaskRowTemplate};
use crate::view::{space_color_options, task_priority_options, task_status_options, SpaceView, TaskView};

pub async fn dashboard(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let active = sqlx::query_as::<_, Space>(
        "SELECT * FROM spaces WHERE user_id = ? AND archived_at IS NULL ORDER BY created_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let archived = sqlx::query_as::<_, Space>(
        "SELECT * FROM spaces WHERE user_id = ? AND archived_at IS NOT NULL ORDER BY archived_at DESC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(DashboardTemplate {
        display_name: user.display_name,
        active_spaces: active.into_iter().map(SpaceView::from).collect(),
        archived_spaces: archived.into_iter().map(SpaceView::from).collect(),
        color_options: space_color_options(""),
    })
}

#[derive(Deserialize)]
pub struct SpaceForm {
    pub name: String,
    pub color: SpaceColor,
}

pub async fn create_space(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Form(form): Form<SpaceForm>,
) -> Result<impl IntoResponse, AppError> {
    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Space name can't be empty.".into()));
    }
    sqlx::query("INSERT INTO spaces (user_id, name, color) VALUES (?, ?, ?)")
        .bind(user.id)
        .bind(name)
        .bind(form.color)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to("/spaces"))
}

async fn owned_space(pool: &SqlitePool, space_id: i64, user_id: i64) -> Result<Space, AppError> {
    sqlx::query_as::<_, Space>("SELECT * FROM spaces WHERE id = ? AND user_id = ?")
        .bind(space_id)
        .bind(user_id)
        .fetch_optional(pool)
        .await?
        .ok_or(AppError::NotFound)
}

pub async fn space_detail(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(space_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let space = owned_space(&state.pool, space_id, user.id).await?;

    let tasks = sqlx::query_as::<_, Task>(
        r#"SELECT * FROM tasks WHERE space_id = ?
           ORDER BY (status = 'done'), (due_at IS NULL), due_at ASC, created_at DESC"#,
    )
    .bind(space_id)
    .fetch_all(&state.pool)
    .await?;

    let mut tasks_html = String::new();
    for task in tasks {
        let view = TaskView::from(task);
        let status_options = task_status_options(view.status_value);
        let row = TaskRowTemplate { task: view, status_options };
        tasks_html.push_str(&row.render().map_err(|e| anyhow::anyhow!(e))?);
    }

    let space_color = space.color.as_str();
    Ok(SpaceDetailTemplate {
        display_name: user.display_name,
        space: SpaceView::from(space),
        tasks_html,
        color_options: space_color_options(space_color),
        priority_options: task_priority_options("normal"),
    })
}

pub async fn edit_space(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(space_id): Path<i64>,
    Form(form): Form<SpaceForm>,
) -> Result<impl IntoResponse, AppError> {
    owned_space(&state.pool, space_id, user.id).await?;

    let name = form.name.trim().to_string();
    if name.is_empty() {
        return Err(AppError::BadRequest("Space name can't be empty.".into()));
    }
    sqlx::query("UPDATE spaces SET name = ?, color = ? WHERE id = ? AND user_id = ?")
        .bind(name)
        .bind(form.color)
        .bind(space_id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    Ok(Redirect::to(&format!("/spaces/{space_id}")))
}

pub async fn archive_space(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(space_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let space = owned_space(&state.pool, space_id, user.id).await?;
    if space.archived_at.is_some() {
        sqlx::query("UPDATE spaces SET archived_at = NULL WHERE id = ?")
            .bind(space_id)
            .execute(&state.pool)
            .await?;
    } else {
        sqlx::query("UPDATE spaces SET archived_at = ? WHERE id = ?")
            .bind(chrono::Utc::now())
            .bind(space_id)
            .execute(&state.pool)
            .await?;
    }
    Ok(Redirect::to("/spaces"))
}

pub async fn delete_space(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(space_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM spaces WHERE id = ? AND user_id = ?")
        .bind(space_id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to("/spaces"))
}

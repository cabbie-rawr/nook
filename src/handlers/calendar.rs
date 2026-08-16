//! A minimal calendar: the standing weekly/one-off blocks that Focus and Up
//! Next read from. Not a full calendar view — just enough to populate a
//! schedule.

use axum::extract::{Path, State};
use axum::response::{IntoResponse, Redirect};
use axum::Form;
use chrono::NaiveTime;
use serde::Deserialize;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::models::{Space, ScheduleBlock};
use crate::state::AppState;
use crate::templates::CalendarTemplate;
use crate::view::space_color_options;

const WEEKDAY_NAMES: [&str; 7] = ["Sunday", "Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday"];

pub async fn show(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    let recurring = sqlx::query_as::<_, ScheduleBlock>(
        "SELECT * FROM schedule_blocks WHERE user_id = $1 AND recurring = true ORDER BY day_of_week ASC, start_time ASC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let one_off = sqlx::query_as::<_, ScheduleBlock>(
        "SELECT * FROM schedule_blocks WHERE user_id = $1 AND recurring = false AND specific_date >= CURRENT_DATE
         ORDER BY specific_date ASC, start_time ASC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let spaces = sqlx::query_as::<_, Space>(
        "SELECT * FROM spaces WHERE user_id = $1 AND archived_at IS NULL ORDER BY name ASC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    Ok(CalendarTemplate {
        display_name: user.display_name,
        recurring: recurring.into_iter().map(|b| block_view(b)).collect(),
        one_off: one_off.into_iter().map(|b| block_view(b)).collect(),
        spaces: spaces.into_iter().map(|s| (s.id, s.name)).collect(),
        color_options: space_color_options(""),
    })
}

pub struct BlockView {
    pub id: i64,
    pub title: String,
    pub time_range: String,
    pub day_label: String,
}

fn block_view(block: ScheduleBlock) -> BlockView {
    let day_label = match block.day_of_week {
        Some(d) if (0..7).contains(&d) => WEEKDAY_NAMES[d as usize].to_string(),
        _ => block.specific_date.map(|d| d.format("%b %-d").to_string()).unwrap_or_default(),
    };
    BlockView {
        id: block.id,
        title: block.title,
        time_range: format!("{}–{}", block.start_time.format("%-I:%M %p"), block.end_time.format("%-I:%M %p")),
        day_label,
    }
}

#[derive(Deserialize)]
pub struct BlockForm {
    pub title: String,
    pub kind: String, // "recurring" | "one_off"
    #[serde(default)]
    pub day_of_week: Option<i64>,
    #[serde(default)]
    pub specific_date: String,
    pub start_time: String,
    pub end_time: String,
    #[serde(default)]
    pub space_id: Option<i64>,
}

pub async fn create(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Form(form): Form<BlockForm>,
) -> Result<impl IntoResponse, AppError> {
    let title = form.title.trim().to_string();
    if title.is_empty() {
        return Err(AppError::BadRequest("Title can't be empty.".into()));
    }
    let start = NaiveTime::parse_from_str(form.start_time.trim(), "%H:%M")
        .map_err(|_| AppError::BadRequest("Invalid start time.".into()))?;
    let end = NaiveTime::parse_from_str(form.end_time.trim(), "%H:%M")
        .map_err(|_| AppError::BadRequest("Invalid end time.".into()))?;
    if end <= start {
        return Err(AppError::BadRequest("End time must be after start time.".into()));
    }

    let recurring = form.kind == "recurring";
    let (day_of_week, specific_date) = if recurring {
        let day = form.day_of_week.filter(|d| (0..7).contains(d)).ok_or_else(|| {
            AppError::BadRequest("Pick a day of the week.".into())
        })?;
        (Some(day), None)
    } else {
        let date = chrono::NaiveDate::parse_from_str(form.specific_date.trim(), "%Y-%m-%d")
            .map_err(|_| AppError::BadRequest("Invalid date.".into()))?;
        (None, Some(date))
    };

    if let Some(space_id) = form.space_id {
        let owns = sqlx::query_scalar::<_, i64>("SELECT id FROM spaces WHERE id = $1 AND user_id = $2")
            .bind(space_id)
            .bind(user.id)
            .fetch_optional(&state.pool)
            .await?;
        if owns.is_none() {
            return Err(AppError::NotFound);
        }
    }

    sqlx::query(
        "INSERT INTO schedule_blocks (user_id, space_id, title, day_of_week, start_time, end_time, recurring, specific_date)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
    )
    .bind(user.id)
    .bind(form.space_id)
    .bind(title)
    .bind(day_of_week)
    .bind(start)
    .bind(end)
    .bind(recurring)
    .bind(specific_date)
    .execute(&state.pool)
    .await?;

    Ok(Redirect::to("/calendar"))
}

pub async fn delete(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Path(block_id): Path<i64>,
) -> Result<impl IntoResponse, AppError> {
    let result = sqlx::query("DELETE FROM schedule_blocks WHERE id = $1 AND user_id = $2")
        .bind(block_id)
        .bind(user.id)
        .execute(&state.pool)
        .await?;
    if result.rows_affected() == 0 {
        return Err(AppError::NotFound);
    }
    Ok(Redirect::to("/calendar"))
}

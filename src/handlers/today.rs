//! The Today home screen: a shell that renders skeleton cards, each of which
//! lazy-loads its own partial (`hx-trigger="load"`) so a slow card can never
//! block a fast one. Every partial handler renders its own error/empty state
//! rather than propagating a 500 — a failed card degrades gracefully in place
//! via the `respond` wrapper below.

use askama::Template;
use axum::extract::{Query, State};
use axum::http::header;
use axum::response::{Html, IntoResponse};
use chrono::{DateTime, Datelike, Duration, NaiveDate, NaiveTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::future::Future;

use crate::auth::CurrentUser;
use crate::error::AppError;
use crate::models::{SpaceColor, User};
use crate::state::AppState;
use crate::templates::{
    DueSoonTemplate, FocusTemplate, GettingStartedTemplate, JumpBackInTemplate, MomentumTemplate,
    SpaceProgressTemplate, TodayTemplate, UpNextTemplate,
};
use crate::view::{format_ago, format_relative_due};

pub const DEFAULT_ORDER: [&str; 7] =
    ["focus", "up_next", "due_soon", "momentum", "jump_back_in", "space_progress", "getting_started"];

/// Runs a card's data+render future; on any failure, logs it and swaps in a
/// small inline error card with a retry button instead of a page-wide 500.
async fn respond<F>(key: &'static str, label: &'static str, fut: F) -> Html<String>
where
    F: Future<Output = Result<String, AppError>>,
{
    match fut.await {
        Ok(html) => Html(html),
        Err(err) => {
            tracing::error!(?err, card = key, "card failed to load");
            Html(error_card(key, label))
        }
    }
}

fn error_card(key: &str, label: &str) -> String {
    format!(
        r#"<section class="card card-error" data-card="{key}" tabindex="0" aria-labelledby="card-{key}-label">
  <h2 class="card-label" id="card-{key}-label">{label}</h2>
  <div class="card-body">
    <p class="empty-state">Couldn't load this right now.</p>
    <button class="btn-secondary" hx-get="/partials/{key}" hx-target="closest .card" hx-swap="outerHTML" type="button">Retry</button>
  </div>
</section>"#
    )
}

pub async fn shell(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let stored = sqlx::query_scalar::<_, String>("SELECT layout FROM user_layout WHERE user_id = $1")
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?;

    let mut order: Vec<String> = stored.and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok()).unwrap_or_default();
    order.retain(|k| DEFAULT_ORDER.contains(&k.as_str()));
    for key in DEFAULT_ORDER {
        if !order.iter().any(|k| k == key) {
            order.push(key.to_string());
        }
    }

    #[derive(Serialize)]
    struct SpaceOption {
        id: i64,
        name: String,
        color: &'static str,
    }
    let spaces = sqlx::query_as::<_, (i64, String, SpaceColor)>(
        "SELECT id, name, color FROM spaces WHERE user_id = $1 AND archived_at IS NULL ORDER BY name ASC",
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;
    let spaces_json = serde_json::to_string(
        &spaces.into_iter().map(|(id, name, color)| SpaceOption { id, name, color: color.as_str() }).collect::<Vec<_>>(),
    )
    .unwrap_or_else(|_| "[]".to_string());

    Ok(TodayTemplate { display_name: user.display_name, order, spaces_json })
}

fn first_name(display_name: &str) -> &str {
    display_name.split_whitespace().next().unwrap_or(display_name)
}

fn greeting(name: &str, now: DateTime<Utc>) -> String {
    let part = match now.hour() {
        0..=11 => "morning",
        12..=17 => "afternoon",
        _ => "evening",
    };
    format!("Good {part}, {name}")
}

// ---------------------------------------------------------------------------
// Focus
// ---------------------------------------------------------------------------

pub struct FocusView {
    pub kind: &'static str, // "schedule" | "task" | "none"
    pub title: String,
    pub subtitle: String,
    pub task_id: Option<i64>,
    pub space_color: Option<&'static str>,
}

pub async fn focus(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("focus", "Focus", focus_inner(user, state)).await
}

async fn focus_inner(user: User, state: AppState) -> Result<String, AppError> {
    let now = Utc::now();
    let today = now.date_naive();
    let weekday = today.weekday().num_days_from_sunday() as i64;
    let now_time = now.time();

    let active_block = sqlx::query_as::<_, (String, NaiveTime, Option<SpaceColor>)>(
        r#"SELECT schedule_blocks.title, schedule_blocks.end_time, spaces.color
           FROM schedule_blocks LEFT JOIN spaces ON spaces.id = schedule_blocks.space_id
           WHERE schedule_blocks.user_id = $1 AND schedule_blocks.start_time <= $2 AND schedule_blocks.end_time >= $3
             AND ((schedule_blocks.recurring = true AND schedule_blocks.day_of_week = $4)
                  OR (schedule_blocks.recurring = false AND schedule_blocks.specific_date = $5))
           LIMIT 1"#,
    )
    .bind(user.id)
    .bind(now_time)
    .bind(now_time)
    .bind(weekday)
    .bind(today)
    .fetch_optional(&state.pool)
    .await?;

    let focus = if let Some((title, end, color)) = active_block {
        FocusView {
            kind: "schedule",
            title,
            subtitle: format!("Until {}", end.format("%-I:%M %p")),
            task_id: None,
            space_color: color.map(|c| c.as_str()),
        }
    } else {
        let nearest = sqlx::query_as::<_, (i64, String, SpaceColor, DateTime<Utc>)>(
            r#"SELECT tasks.id, tasks.title, spaces.color, tasks.due_at FROM tasks
               JOIN spaces ON spaces.id = tasks.space_id
               WHERE spaces.user_id = $1 AND tasks.status != 'done' AND tasks.due_at IS NOT NULL
               ORDER BY tasks.due_at ASC LIMIT 1"#,
        )
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?;

        match nearest {
            Some((id, title, color, due)) => FocusView {
                kind: "task",
                title,
                subtitle: format_relative_due(due, now),
                task_id: Some(id),
                space_color: Some(color.as_str()),
            },
            None => {
                FocusView { kind: "none", title: String::new(), subtitle: String::new(), task_id: None, space_color: None }
            }
        }
    };

    let html = FocusTemplate {
        greeting: greeting(first_name(&user.display_name), now),
        date_str: now.format("%A, %B %-d").to_string(),
        focus,
    }
    .render()
    .map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Up Next
// ---------------------------------------------------------------------------

pub struct UpNextItemView {
    pub time_label: String,
    pub title: String,
    pub space_color: &'static str,
    pub is_past: bool,
    pub kind: &'static str,
    sort_time: NaiveTime,
}

pub async fn up_next(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("up_next", "Up Next", up_next_inner(user, state)).await
}

async fn up_next_inner(user: User, state: AppState) -> Result<String, AppError> {
    let now = Utc::now();
    let today = now.date_naive();
    let weekday = today.weekday().num_days_from_sunday() as i64;

    let blocks = sqlx::query_as::<_, (String, NaiveTime, NaiveTime, Option<SpaceColor>)>(
        r#"SELECT schedule_blocks.title, schedule_blocks.start_time, schedule_blocks.end_time, spaces.color
           FROM schedule_blocks LEFT JOIN spaces ON spaces.id = schedule_blocks.space_id
           WHERE schedule_blocks.user_id = $1
             AND ((schedule_blocks.recurring = true AND schedule_blocks.day_of_week = $2)
                  OR (schedule_blocks.recurring = false AND schedule_blocks.specific_date = $3))"#,
    )
    .bind(user.id)
    .bind(weekday)
    .bind(today)
    .fetch_all(&state.pool)
    .await?;

    let due_today = sqlx::query_as::<_, (String, DateTime<Utc>, SpaceColor)>(
        r#"SELECT tasks.title, tasks.due_at, spaces.color FROM tasks
           JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 AND tasks.status != 'done' AND tasks.due_at IS NOT NULL
             AND DATE(tasks.due_at) = DATE($2)"#,
    )
    .bind(user.id)
    .bind(now)
    .fetch_all(&state.pool)
    .await?;

    let mut items: Vec<UpNextItemView> = Vec::new();
    for (title, start, end, color) in blocks {
        let end_dt = today.and_time(end).and_utc();
        items.push(UpNextItemView {
            time_label: start.format("%-I:%M %p").to_string(),
            title,
            space_color: color.map(|c| c.as_str()).unwrap_or("slate"),
            is_past: end_dt < now,
            kind: "block",
            sort_time: start,
        });
    }
    for (title, due, color) in due_today {
        items.push(UpNextItemView {
            time_label: due.format("%-I:%M %p").to_string(),
            title,
            space_color: color.as_str(),
            is_past: due < now,
            kind: "deadline",
            sort_time: due.time(),
        });
    }
    items.sort_by_key(|i| i.sort_time);
    let now_line_index = items.iter().position(|i| !i.is_past).unwrap_or(items.len());

    let html = UpNextTemplate { items, now_line_index }.render().map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Due Soon
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct DueSoonRaw {
    id: i64,
    title: String,
    due_at: DateTime<Utc>,
    space_color: SpaceColor,
    attachment_count: i64,
}

pub struct DueRowView {
    pub id: i64,
    pub title: String,
    pub space_color: &'static str,
    pub relative: String,
    pub attachment_count: i64,
}

pub struct DueGroupView {
    pub key: &'static str,
    pub label: &'static str,
    pub rows: Vec<DueRowView>,
}

pub async fn due_soon(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("due_soon", "Due Soon", due_soon_inner(user, state)).await
}

async fn due_soon_inner(user: User, state: AppState) -> Result<String, AppError> {
    let now = Utc::now();
    let horizon = now + Duration::days(7);

    let rows = sqlx::query_as::<_, DueSoonRaw>(
        r#"SELECT tasks.id, tasks.title, tasks.due_at, spaces.color as space_color,
                  (SELECT COUNT(*) FROM attachments WHERE attachments.task_id = tasks.id) as attachment_count
           FROM tasks JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 AND tasks.status != 'done' AND tasks.due_at IS NOT NULL AND tasks.due_at <= $2
           ORDER BY tasks.due_at ASC"#,
    )
    .bind(user.id)
    .bind(horizon)
    .fetch_all(&state.pool)
    .await?;

    let today = now.date_naive();
    let tomorrow = today + Duration::days(1);
    let mut overdue = Vec::new();
    let mut today_rows = Vec::new();
    let mut tomorrow_rows = Vec::new();
    let mut week_rows = Vec::new();

    for r in rows {
        let row = DueRowView {
            id: r.id,
            title: r.title,
            space_color: r.space_color.as_str(),
            relative: format_relative_due(r.due_at, now),
            attachment_count: r.attachment_count,
        };
        if r.due_at < now {
            overdue.push(row);
        } else if r.due_at.date_naive() == today {
            today_rows.push(row);
        } else if r.due_at.date_naive() == tomorrow {
            tomorrow_rows.push(row);
        } else {
            week_rows.push(row);
        }
    }

    let mut groups = Vec::new();
    if !overdue.is_empty() {
        groups.push(DueGroupView { key: "overdue", label: "Overdue", rows: overdue });
    }
    if !today_rows.is_empty() {
        groups.push(DueGroupView { key: "today", label: "Today", rows: today_rows });
    }
    if !tomorrow_rows.is_empty() {
        groups.push(DueGroupView { key: "tomorrow", label: "Tomorrow", rows: tomorrow_rows });
    }
    if !week_rows.is_empty() {
        groups.push(DueGroupView { key: "week", label: "This week", rows: week_rows });
    }

    let html = DueSoonTemplate { groups }.render().map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Momentum
// ---------------------------------------------------------------------------

pub struct MomentumDay {
    pub date_label: String,
    pub count: i64,
    pub level: u8,
    pub in_future: bool,
}

pub async fn momentum(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("momentum", "Momentum", momentum_inner(user, state)).await
}

async fn momentum_inner(user: User, state: AppState) -> Result<String, AppError> {
    let now = Utc::now();
    let today = now.date_naive();
    let start = today - Duration::days(83);

    // `DATE(...)` returns a Postgres `date`, decoded straight into `NaiveDate`
    // — no manual `%Y-%m-%d` parsing needed the way SQLite's TEXT return did.
    let rows = sqlx::query_as::<_, (NaiveDate, i64)>(
        r#"SELECT DATE(tasks.completed_at) as d, COUNT(*) as c FROM tasks
           JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 AND tasks.completed_at IS NOT NULL AND DATE(tasks.completed_at) >= $2
           GROUP BY d"#,
    )
    .bind(user.id)
    .bind(start)
    .fetch_all(&state.pool)
    .await?;

    let mut counts: HashMap<NaiveDate, i64> = HashMap::new();
    for (date, c) in rows {
        counts.insert(date, c);
    }

    let grid_start = start - Duration::days(start.weekday().num_days_from_sunday() as i64);
    let mut weeks: Vec<Vec<MomentumDay>> = Vec::new();
    let mut cursor = grid_start;
    for _ in 0..12 {
        let mut week = Vec::new();
        for _ in 0..7 {
            let count = counts.get(&cursor).copied().unwrap_or(0);
            let level = match count {
                0 => 0,
                1 => 1,
                2..=3 => 2,
                4..=6 => 3,
                _ => 4,
            };
            week.push(MomentumDay {
                date_label: cursor.format("%b %-d").to_string(),
                count,
                level,
                in_future: cursor > today,
            });
            cursor += Duration::days(1);
        }
        weeks.push(week);
    }

    let mut streak = 0i64;
    let mut day = today;
    if counts.get(&day).copied().unwrap_or(0) == 0 {
        day -= Duration::days(1);
    }
    while counts.get(&day).copied().unwrap_or(0) > 0 {
        streak += 1;
        day -= Duration::days(1);
    }

    let month_start = today.with_day(1).unwrap_or(today);
    let completed_this_month = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM tasks JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 AND tasks.completed_at IS NOT NULL AND DATE(tasks.completed_at) >= $2"#,
    )
    .bind(user.id)
    .bind(month_start)
    .fetch_one(&state.pool)
    .await?;

    let html = MomentumTemplate { weeks, streak, completed_this_month }.render().map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Jump Back In
// ---------------------------------------------------------------------------

pub struct JumpBackInItemView {
    pub kind: &'static str, // "task" | "file"
    pub task_id: i64,
    pub title: String,
    pub space_name: String,
    pub space_color: &'static str,
    pub when_label: String,
}

pub async fn jump_back_in(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("jump_back_in", "Jump Back In", jump_back_in_inner(user, state)).await
}

async fn jump_back_in_inner(user: User, state: AppState) -> Result<String, AppError> {
    let now = Utc::now();

    let tasks = sqlx::query_as::<_, (i64, String, String, SpaceColor, DateTime<Utc>)>(
        r#"SELECT tasks.id, tasks.title, spaces.name, spaces.color, tasks.last_opened_at
           FROM tasks JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 AND tasks.last_opened_at IS NOT NULL
           ORDER BY tasks.last_opened_at DESC LIMIT 5"#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let files = sqlx::query_as::<_, (i64, i64, String, String, SpaceColor, DateTime<Utc>)>(
        r#"SELECT attachments.id, attachments.task_id, attachments.original_filename, spaces.name, spaces.color, attachments.uploaded_at
           FROM attachments JOIN tasks ON tasks.id = attachments.task_id JOIN spaces ON spaces.id = tasks.space_id
           WHERE spaces.user_id = $1 ORDER BY attachments.uploaded_at DESC LIMIT 5"#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let mut items: Vec<(DateTime<Utc>, JumpBackInItemView)> = Vec::new();
    for (id, title, space_name, color, opened_at) in tasks {
        items.push((
            opened_at,
            JumpBackInItemView {
                kind: "task",
                task_id: id,
                title,
                space_name,
                space_color: color.as_str(),
                when_label: format_ago(opened_at, now),
            },
        ));
    }
    for (_id, task_id, filename, space_name, color, uploaded_at) in files {
        items.push((
            uploaded_at,
            JumpBackInItemView {
                kind: "file",
                task_id,
                title: filename,
                space_name,
                space_color: color.as_str(),
                when_label: format_ago(uploaded_at, now),
            },
        ));
    }
    items.sort_by(|a, b| b.0.cmp(&a.0));
    items.truncate(5);

    let html = JumpBackInTemplate { items: items.into_iter().map(|(_, v)| v).collect() }
        .render()
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Space Progress
// ---------------------------------------------------------------------------

#[derive(sqlx::FromRow)]
struct SpaceProgressRaw {
    id: i64,
    name: String,
    color: SpaceColor,
    total: i64,
    done: i64,
}

pub struct SpaceProgressRow {
    pub id: i64,
    pub name: String,
    pub color: &'static str,
    pub done: i64,
    pub open: i64,
    pub percent: i64,
}

pub async fn space_progress(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("space_progress", "Space Progress", space_progress_inner(user, state)).await
}

async fn space_progress_inner(user: User, state: AppState) -> Result<String, AppError> {
    let rows = sqlx::query_as::<_, SpaceProgressRaw>(
        r#"SELECT spaces.id, spaces.name, spaces.color,
                  COUNT(tasks.id) as total,
                  COALESCE(SUM(CASE WHEN tasks.status = 'done' THEN 1 ELSE 0 END), 0) as done
           FROM spaces LEFT JOIN tasks ON tasks.space_id = spaces.id
           WHERE spaces.user_id = $1 AND spaces.archived_at IS NULL
           GROUP BY spaces.id ORDER BY spaces.created_at DESC"#,
    )
    .bind(user.id)
    .fetch_all(&state.pool)
    .await?;

    let rows = rows
        .into_iter()
        .map(|r| SpaceProgressRow {
            id: r.id,
            name: r.name,
            color: r.color.as_str(),
            done: r.done,
            open: r.total - r.done,
            percent: if r.total > 0 { r.done * 100 / r.total } else { 0 },
        })
        .collect();

    let html = SpaceProgressTemplate { rows }.render().map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

// ---------------------------------------------------------------------------
// Getting Started
// ---------------------------------------------------------------------------

pub async fn getting_started(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> impl IntoResponse {
    respond("getting_started", "Getting Started", getting_started_inner(user, state)).await
}

async fn getting_started_inner(user: User, state: AppState) -> Result<String, AppError> {
    let space_count = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM spaces WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?;
    let task_count = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasks JOIN spaces ON spaces.id = tasks.space_id WHERE spaces.user_id = $1",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?;
    let has_deadline = sqlx::query_scalar::<_, i64>(
        "SELECT COUNT(*) FROM tasks JOIN spaces ON spaces.id = tasks.space_id WHERE spaces.user_id = $1 AND tasks.due_at IS NOT NULL",
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?
        > 0;
    let has_attachment = sqlx::query_scalar::<_, i64>(
        r#"SELECT COUNT(*) FROM attachments JOIN tasks ON tasks.id = attachments.task_id
           JOIN spaces ON spaces.id = tasks.space_id WHERE spaces.user_id = $1"#,
    )
    .bind(user.id)
    .fetch_one(&state.pool)
    .await?
        > 0;
    let has_schedule = sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM schedule_blocks WHERE user_id = $1")
        .bind(user.id)
        .fetch_one(&state.pool)
        .await?
        > 0;

    let dismissed = sqlx::query_scalar::<_, bool>("SELECT dismissed FROM onboarding_state WHERE user_id = $1")
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or(false);

    let items = vec![
        ("Create your first space".to_string(), space_count > 0),
        ("Add a task with a deadline".to_string(), has_deadline),
        ("Attach a file".to_string(), has_attachment),
        ("Add your schedule".to_string(), has_schedule),
    ];
    let completed_count = items.iter().filter(|(_, done)| *done).count();
    let all_done = completed_count == items.len();
    let eligible = task_count < 3 || space_count < 2;
    let show = eligible && !all_done && !dismissed;

    let html = GettingStartedTemplate { show, total: items.len(), completed_count, items }
        .render()
        .map_err(|e| anyhow::anyhow!(e))?;
    Ok(html)
}

pub async fn dismiss_onboarding(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
) -> Result<impl IntoResponse, AppError> {
    sqlx::query(
        "INSERT INTO onboarding_state (user_id, dismissed) VALUES ($1, true)
         ON CONFLICT (user_id) DO UPDATE SET dismissed = true",
    )
    .bind(user.id)
    .execute(&state.pool)
    .await?;
    Ok(axum::http::StatusCode::OK)
}

// ---------------------------------------------------------------------------
// Search (command palette) + layout persistence
// ---------------------------------------------------------------------------

#[derive(Serialize)]
pub struct SearchResult {
    kind: &'static str,
    id: i64,
    title: String,
    subtitle: String,
    url: String,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    q: String,
}

pub async fn search(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    Query(params): Query<SearchQuery>,
) -> Result<impl IntoResponse, AppError> {
    let query = params.q.trim();
    let mut results = Vec::new();

    if query.is_empty() {
        let recent = sqlx::query_as::<_, (i64, String, String)>(
            r#"SELECT tasks.id, tasks.title, spaces.name FROM tasks
               JOIN spaces ON spaces.id = tasks.space_id
               WHERE spaces.user_id = $1 AND tasks.last_opened_at IS NOT NULL
               ORDER BY tasks.last_opened_at DESC LIMIT 5"#,
        )
        .bind(user.id)
        .fetch_all(&state.pool)
        .await?;
        for (id, title, space_name) in recent {
            results.push(SearchResult { kind: "task", id, title, subtitle: space_name, url: format!("/tasks/{id}") });
        }
    } else {
        let like = format!("%{query}%");
        // ILIKE (not LIKE) to keep the case-insensitive matching SQLite's
        // LIKE gave us for free on ASCII.
        let tasks = sqlx::query_as::<_, (i64, String, String)>(
            r#"SELECT tasks.id, tasks.title, spaces.name FROM tasks
               JOIN spaces ON spaces.id = tasks.space_id
               WHERE spaces.user_id = $1 AND tasks.title ILIKE $2 ORDER BY tasks.updated_at DESC LIMIT 6"#,
        )
        .bind(user.id)
        .bind(&like)
        .fetch_all(&state.pool)
        .await?;
        for (id, title, space_name) in tasks {
            results.push(SearchResult { kind: "task", id, title, subtitle: space_name, url: format!("/tasks/{id}") });
        }

        let spaces = sqlx::query_as::<_, (i64, String)>(
            "SELECT id, name FROM spaces WHERE user_id = $1 AND archived_at IS NULL AND name ILIKE $2 ORDER BY name ASC LIMIT 4",
        )
        .bind(user.id)
        .bind(&like)
        .fetch_all(&state.pool)
        .await?;
        for (id, name) in spaces {
            results.push(SearchResult { kind: "space", id, title: name, subtitle: "Space".into(), url: format!("/spaces/{id}") });
        }
    }

    Ok(axum::Json(results))
}

#[derive(Deserialize)]
pub struct LayoutBody {
    pub order: Vec<String>,
}

pub async fn get_layout(CurrentUser(user): CurrentUser, State(state): State<AppState>) -> Result<impl IntoResponse, AppError> {
    let layout = sqlx::query_scalar::<_, String>("SELECT layout FROM user_layout WHERE user_id = $1")
        .bind(user.id)
        .fetch_optional(&state.pool)
        .await?
        .unwrap_or_else(|| "[]".to_string());
    Ok(([(header::CONTENT_TYPE, "application/json")], layout))
}

pub async fn save_layout(
    CurrentUser(user): CurrentUser,
    State(state): State<AppState>,
    axum::Json(body): axum::Json<LayoutBody>,
) -> Result<impl IntoResponse, AppError> {
    let cleaned: Vec<String> = body.order.into_iter().filter(|k| DEFAULT_ORDER.contains(&k.as_str())).collect();
    let layout = serde_json::to_string(&cleaned).unwrap_or_else(|_| "[]".to_string());
    sqlx::query(
        "INSERT INTO user_layout (user_id, layout, updated_at) VALUES ($1, $2, $3)
         ON CONFLICT (user_id) DO UPDATE SET layout = excluded.layout, updated_at = excluded.updated_at",
    )
    .bind(user.id)
    .bind(layout)
    .bind(Utc::now())
    .execute(&state.pool)
    .await?;
    Ok(axum::http::StatusCode::OK)
}

//! Template-facing view models. Keeping these separate from the `sqlx::FromRow`
//! structs in `models` means templates never need to reach for enum variant
//! paths or do date math — everything they touch is a plain string or bool
//! computed once in Rust.

use chrono::{DateTime, Utc};

use crate::models::{Space, SpaceColor, Task, TaskPriority, TaskStatus};

fn plural(n: i64) -> &'static str {
    if n == 1 {
        ""
    } else {
        "s"
    }
}

/// "in 3 hours" / "overdue by 2 days" — used on Due Soon rows.
pub fn format_relative_due(due: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = due - now;
    if diff.num_seconds() >= 0 {
        format_span(diff, "in ")
    } else {
        format_span(-diff, "overdue by ")
    }
}

fn format_span(d: chrono::Duration, prefix: &str) -> String {
    let minutes = d.num_minutes();
    if minutes < 1 {
        return format!("{prefix}a moment");
    }
    if minutes < 60 {
        return format!("{prefix}{minutes} min{}", plural(minutes));
    }
    let hours = d.num_hours();
    if hours < 24 {
        return format!("{prefix}{hours} hour{}", plural(hours));
    }
    let days = d.num_days();
    format!("{prefix}{days} day{}", plural(days))
}

/// "2h ago" / "yesterday" style relative-past formatting — used on Jump Back In.
pub fn format_ago(when: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let diff = now - when;
    if diff.num_seconds() < 60 {
        return "just now".into();
    }
    if diff.num_minutes() < 60 {
        return format!("{}m ago", diff.num_minutes());
    }
    if diff.num_hours() < 24 {
        return format!("{}h ago", diff.num_hours());
    }
    if diff.num_days() < 7 {
        return format!("{}d ago", diff.num_days());
    }
    when.format("%b %-d").to_string()
}

/// A `<select>` option with `selected` already resolved — comparing enum
/// values inside the template (rather than here) trips Askama's generated
/// equality checks over the borrowed tuple items, so the flag is computed
/// once in Rust instead.
pub struct SelectOption {
    pub value: &'static str,
    pub label: &'static str,
    pub selected: bool,
}

pub fn space_color_options(selected: &str) -> Vec<SelectOption> {
    SpaceColor::ALL
        .iter()
        .map(|c| SelectOption { value: c.as_str(), label: c.label(), selected: c.as_str() == selected })
        .collect()
}

pub fn task_priority_options(selected: &str) -> Vec<SelectOption> {
    TaskPriority::ALL
        .iter()
        .map(|p| SelectOption { value: p.as_str(), label: p.label(), selected: p.as_str() == selected })
        .collect()
}

pub fn task_status_options(selected: &str) -> Vec<SelectOption> {
    TaskStatus::ALL
        .iter()
        .map(|s| SelectOption { value: s.as_str(), label: s.label(), selected: s.as_str() == selected })
        .collect()
}

pub struct SpaceView {
    pub id: i64,
    pub name: String,
    pub color: &'static str,
    pub icon: String,
    pub archived: bool,
}

impl From<Space> for SpaceView {
    fn from(space: Space) -> Self {
        SpaceView {
            id: space.id,
            name: space.name,
            color: space.color.as_str(),
            icon: space.icon,
            archived: space.archived_at.is_some(),
        }
    }
}

pub struct TaskView {
    pub id: i64,
    pub title: String,
    pub notes: String,
    pub status_value: &'static str,
    pub status_label: &'static str,
    pub is_done: bool,
    pub priority_value: &'static str,
    pub priority_label: &'static str,
    pub due_label: Option<String>,
    pub overdue: bool,
    pub logged_minutes: i64,
}

impl From<Task> for TaskView {
    fn from(task: Task) -> Self {
        let overdue = task.status != crate::models::TaskStatus::Done
            && task.due_at.is_some_and(|due| due < Utc::now());

        TaskView {
            id: task.id,
            title: task.title,
            notes: task.notes,
            status_value: task.status.as_str(),
            status_label: task.status.label(),
            is_done: task.status == crate::models::TaskStatus::Done,
            priority_value: task.priority.as_str(),
            priority_label: task.priority.label(),
            due_label: task.due_at.map(|d| d.format("%b %-d, %Y").to_string()),
            overdue,
            logged_minutes: task.logged_minutes,
        }
    }
}

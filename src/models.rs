//! Row types for every table in `migrations/0001_initial_schema.sql`.
//!
//! Enums derive `sqlx::Type` and are stored as their (snake_case) variant name
//! in a `TEXT` column — this matches the `CHECK (col IN (...))` constraints in
//! the migration exactly, so an invalid string can't reach the database from
//! either direction. Timestamps are `DateTime<Utc>` stored as ISO-8601 TEXT;
//! `start_time`/`end_time` on schedule blocks are wall-clock `NaiveTime`
//! ("HH:MM"), deliberately not tied to a date.

use chrono::{DateTime, NaiveDate, NaiveTime, Utc};
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum UserMode {
    Student,
    Work,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum ThemePreference {
    Light,
    Dark,
    System,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum SpaceColor {
    Clay,
    Sage,
    Slate,
    Amber,
    Plum,
    Teal,
    Rust,
    Denim,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Todo,
    InProgress,
    Blocked,
    Done,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, sqlx::Type)]
#[sqlx(rename_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Low,
    Normal,
    High,
}

// ---------------------------------------------------------------------------
// Tables
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct User {
    pub id: i64,
    pub email: String,
    #[serde(skip_serializing)]
    pub password_hash: String,
    pub display_name: String,
    pub mode: UserMode,
    pub timezone: String,
    pub theme_preference: ThemePreference,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Session {
    pub id: String,
    pub user_id: i64,
    pub created_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Space {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
    pub color: SpaceColor,
    pub icon: String,
    pub archived_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Task {
    pub id: i64,
    pub space_id: i64,
    pub title: String,
    pub notes: String,
    pub status: TaskStatus,
    pub priority: TaskPriority,
    pub due_at: Option<DateTime<Utc>>,
    pub estimated_minutes: Option<i64>,
    pub completed_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct PlanStep {
    pub id: i64,
    pub task_id: i64,
    pub position: i64,
    pub text: String,
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Attachment {
    pub id: i64,
    pub task_id: i64,
    pub original_filename: String,
    #[serde(skip_serializing)] // never expose the on-disk path to the client
    pub stored_path: String,
    pub mime_type: String,
    pub size_bytes: i64,
    pub uploaded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ScheduleBlock {
    pub id: i64,
    pub user_id: i64,
    pub space_id: Option<i64>,
    pub title: String,
    pub day_of_week: Option<i64>, // 0=Sunday .. 6=Saturday, set when recurring
    pub start_time: NaiveTime,
    pub end_time: NaiveTime,
    pub recurring: bool,
    pub specific_date: Option<NaiveDate>, // set when NOT recurring
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct Tag {
    pub id: i64,
    pub user_id: i64,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct TaskTag {
    pub task_id: i64,
    pub tag_id: i64,
}

// ---------------------------------------------------------------------------
// Attachment upload limit — referenced by both the multipart handler and the
// client-side hint in the upload zone, so the number lives in exactly one place.
// ---------------------------------------------------------------------------
pub const MAX_ATTACHMENT_BYTES: i64 = 25 * 1024 * 1024; // 25 MB

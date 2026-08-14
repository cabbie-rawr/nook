-- Nook — initial schema
-- SQLite. Timestamps are stored as ISO-8601 TEXT (UTC). Booleans are INTEGER 0/1.

PRAGMA foreign_keys = ON;

-- ---------------------------------------------------------------------------
-- users
-- ---------------------------------------------------------------------------
CREATE TABLE users (
    id                INTEGER PRIMARY KEY AUTOINCREMENT,
    email             TEXT NOT NULL UNIQUE,
    password_hash     TEXT NOT NULL,
    display_name      TEXT NOT NULL,
    mode              TEXT NOT NULL DEFAULT 'student'
                          CHECK (mode IN ('student', 'work')),
    timezone          TEXT NOT NULL DEFAULT 'UTC',
    theme_preference  TEXT NOT NULL DEFAULT 'system'
                          CHECK (theme_preference IN ('light', 'dark', 'system')),
    created_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- ---------------------------------------------------------------------------
-- sessions — session-based auth. The cookie carries `id`; everything else
-- (user identity, expiry) is looked up server-side, never trusted from the client.
-- ---------------------------------------------------------------------------
CREATE TABLE sessions (
    id          TEXT PRIMARY KEY,               -- random 256-bit token, hex-encoded
    user_id     INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    expires_at  TEXT NOT NULL
);

CREATE INDEX idx_sessions_user ON sessions(user_id);
CREATE INDEX idx_sessions_expires ON sessions(expires_at);

-- ---------------------------------------------------------------------------
-- spaces — "Subjects" (student mode) / "Projects" (work mode). Vocabulary only
-- changes in the presentation layer; the row shape is identical for both modes.
-- ---------------------------------------------------------------------------
CREATE TABLE spaces (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id       INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name          TEXT NOT NULL,
    color         TEXT NOT NULL
                      CHECK (color IN ('clay','sage','slate','amber','plum','teal','rust','denim')),
    icon          TEXT NOT NULL DEFAULT 'folder',
    archived_at   TEXT,                          -- NULL while active
    created_at    TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_spaces_user ON spaces(user_id);
CREATE INDEX idx_spaces_user_archived ON spaces(user_id, archived_at);

-- ---------------------------------------------------------------------------
-- tasks
-- ---------------------------------------------------------------------------
CREATE TABLE tasks (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    space_id            INTEGER NOT NULL REFERENCES spaces(id) ON DELETE CASCADE,
    title               TEXT NOT NULL,
    notes               TEXT NOT NULL DEFAULT '',
    status              TEXT NOT NULL DEFAULT 'todo'
                            CHECK (status IN ('todo','in_progress','blocked','done')),
    priority            TEXT NOT NULL DEFAULT 'normal'
                            CHECK (priority IN ('low','normal','high')),
    due_at              TEXT,                     -- nullable, ISO-8601 UTC
    estimated_minutes   INTEGER,
    completed_at        TEXT,
    created_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now')),
    updated_at          TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_tasks_space ON tasks(space_id);
CREATE INDEX idx_tasks_space_status ON tasks(space_id, status);
CREATE INDEX idx_tasks_due ON tasks(due_at);
CREATE INDEX idx_tasks_status_due ON tasks(status, due_at);

-- ---------------------------------------------------------------------------
-- plan_steps — a task's ordered, checkable working plan
-- ---------------------------------------------------------------------------
CREATE TABLE plan_steps (
    id        INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id   INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    position  INTEGER NOT NULL,
    text      TEXT NOT NULL,
    done      INTEGER NOT NULL DEFAULT 0 CHECK (done IN (0, 1))
);

CREATE INDEX idx_plan_steps_task_position ON plan_steps(task_id, position);

-- ---------------------------------------------------------------------------
-- attachments — original_filename is what the user sees; stored_path is a
-- generated name on disk outside the web root, so the two are never conflated.
-- ---------------------------------------------------------------------------
CREATE TABLE attachments (
    id                 INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id            INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    original_filename  TEXT NOT NULL,
    stored_path        TEXT NOT NULL UNIQUE,
    mime_type          TEXT NOT NULL,
    size_bytes         INTEGER NOT NULL,
    uploaded_at        TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_attachments_task ON attachments(task_id);

-- ---------------------------------------------------------------------------
-- schedule_blocks — classes/shifts/standing meetings (recurring by day_of_week)
-- or one-off blocks pinned to specific_date. Exactly one of the two is set,
-- enforced in application code (SQLite CHECK constraints can't easily express
-- "exactly one of A/B is null" alongside the recurring flag in a portable way,
-- so the invariant lives in the handler/service layer, covered by tests).
-- ---------------------------------------------------------------------------
CREATE TABLE schedule_blocks (
    id             INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id        INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    space_id       INTEGER REFERENCES spaces(id) ON DELETE SET NULL,
    title          TEXT NOT NULL,
    day_of_week    INTEGER CHECK (day_of_week BETWEEN 0 AND 6), -- 0=Sunday, set when recurring
    start_time     TEXT NOT NULL,                                -- "HH:MM", 24h
    end_time       TEXT NOT NULL,
    recurring      INTEGER NOT NULL DEFAULT 1 CHECK (recurring IN (0, 1)),
    specific_date  TEXT,                                         -- "YYYY-MM-DD", set when NOT recurring
    created_at     TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

CREATE INDEX idx_schedule_blocks_user ON schedule_blocks(user_id);
CREATE INDEX idx_schedule_blocks_user_day ON schedule_blocks(user_id, day_of_week);
CREATE INDEX idx_schedule_blocks_user_date ON schedule_blocks(user_id, specific_date);

-- ---------------------------------------------------------------------------
-- tags / task_tags
-- ---------------------------------------------------------------------------
CREATE TABLE tags (
    id       INTEGER PRIMARY KEY AUTOINCREMENT,
    user_id  INTEGER NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    UNIQUE (user_id, name)
);

CREATE TABLE task_tags (
    task_id  INTEGER NOT NULL REFERENCES tasks(id) ON DELETE CASCADE,
    tag_id   INTEGER NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
    PRIMARY KEY (task_id, tag_id)
);

CREATE INDEX idx_task_tags_tag ON task_tags(tag_id);

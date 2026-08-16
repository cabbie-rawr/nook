-- Nook — Today home screen support
-- Adds task-open tracking + logged focus minutes, per-user bento layout order,
-- and a dismiss flag for the getting-started checklist.

ALTER TABLE tasks ADD COLUMN last_opened_at TEXT;
ALTER TABLE tasks ADD COLUMN logged_minutes INTEGER NOT NULL DEFAULT 0;

-- ---------------------------------------------------------------------------
-- user_layout — the drag-to-reorder order of Today's bento cards, one row per
-- user. `layout` is a JSON array of card keys, e.g. ["focus","up_next",...].
-- ---------------------------------------------------------------------------
CREATE TABLE user_layout (
    user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    layout      TEXT NOT NULL DEFAULT '[]',
    updated_at  TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

-- ---------------------------------------------------------------------------
-- onboarding_state — whether the Getting Started card has been dismissed.
-- Absence of a row means "not dismissed"; completion is derived from task/
-- space counts, not stored here.
-- ---------------------------------------------------------------------------
CREATE TABLE onboarding_state (
    user_id     INTEGER PRIMARY KEY REFERENCES users(id) ON DELETE CASCADE,
    dismissed   INTEGER NOT NULL DEFAULT 0 CHECK (dismissed IN (0, 1))
);

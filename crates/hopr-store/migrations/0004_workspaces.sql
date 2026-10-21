-- Phase 3e: saved layouts (tabs, splits, what each pane connects to). Never secrets.
CREATE TABLE workspaces (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    layout_json TEXT NOT NULL,
    is_autosave INTEGER NOT NULL DEFAULT 0,
    is_startup  INTEGER NOT NULL DEFAULT 0,
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);

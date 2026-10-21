-- Phase 5b: snippets (R1) and named secrets they can use (R10).
CREATE TABLE snippets (
    id          TEXT PRIMARY KEY,
    name        TEXT NOT NULL,
    folder      TEXT NOT NULL DEFAULT '',
    tags_json   TEXT NOT NULL DEFAULT '[]',
    description TEXT NOT NULL DEFAULT '',
    body        TEXT NOT NULL,
    run_mode    TEXT NOT NULL DEFAULT 'paste',
    created_at  TEXT NOT NULL,
    updated_at  TEXT NOT NULL
);
CREATE INDEX snippets_folder ON snippets(folder, name);

-- `{{secret:name}}` values, encrypted under the vault key (AAD "snippet.secret:<name>").
CREATE TABLE snippet_secrets (
    name       TEXT PRIMARY KEY,
    value_enc  BLOB NOT NULL,
    updated_at TEXT NOT NULL
);

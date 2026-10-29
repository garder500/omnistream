CREATE TABLE stream_presets (
    id       TEXT PRIMARY KEY,
    name     TEXT NOT NULL,
    title    TEXT NOT NULL DEFAULT '',
    category TEXT NOT NULL DEFAULT '',
    tags     TEXT NOT NULL DEFAULT '[]'
);

CREATE TABLE accounts (
    id           TEXT PRIMARY KEY,
    platform     TEXT NOT NULL,
    display_name TEXT NOT NULL
);

CREATE TABLE channels (
    id           TEXT PRIMARY KEY,
    platform     TEXT NOT NULL,
    external_id  TEXT NOT NULL,
    display_name TEXT NOT NULL,
    account_id   TEXT REFERENCES accounts(id) ON DELETE SET NULL
);

CREATE TABLE stream_destinations (
    id         TEXT PRIMARY KEY,
    platform   TEXT NOT NULL,
    name       TEXT NOT NULL,
    rtmp_url   TEXT NOT NULL,
    stream_key TEXT NOT NULL,
    enabled    INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE chat_messages (
    id           TEXT PRIMARY KEY,
    channel_id   TEXT NOT NULL REFERENCES channels(id) ON DELETE CASCADE,
    author_id    TEXT NOT NULL,
    author_name  TEXT NOT NULL,
    text         TEXT NOT NULL,
    timestamp_ms INTEGER NOT NULL
);
CREATE INDEX idx_chat_messages_channel_time ON chat_messages(channel_id, timestamp_ms);

CREATE TABLE settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

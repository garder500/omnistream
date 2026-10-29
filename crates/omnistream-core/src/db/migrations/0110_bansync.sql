-- Bans synchronisés : correspondance d'identités entre plateformes, jobs et file d'attente.
CREATE TABLE IF NOT EXISTS identity_links (
    person       TEXT NOT NULL,
    platform     TEXT NOT NULL,
    user_id      TEXT NOT NULL,
    display_name TEXT NOT NULL,
    PRIMARY KEY (platform, user_id)
);
CREATE INDEX IF NOT EXISTS idx_identity_links_person ON identity_links(person);

CREATE TABLE IF NOT EXISTS ban_jobs (
    id            TEXT PRIMARY KEY,
    kind          TEXT NOT NULL,
    source_name   TEXT NOT NULL,
    reason        TEXT,
    created_at_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS ban_queue (
    id            INTEGER PRIMARY KEY AUTOINCREMENT,
    job_id        TEXT NOT NULL REFERENCES ban_jobs(id) ON DELETE CASCADE,
    channel_id    TEXT NOT NULL,
    channel_name  TEXT NOT NULL,
    platform      TEXT NOT NULL,
    user_id       TEXT NOT NULL,
    status        TEXT NOT NULL DEFAULT 'pending',
    attempts      INTEGER NOT NULL DEFAULT 0,
    last_error    TEXT,
    updated_at_ms INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_ban_queue_job ON ban_queue(job_id);

-- Historique des sessions de live : une session = une période pendant laquelle
-- au moins une chaîne suivie est en direct.
CREATE TABLE sessions (
    id            TEXT PRIMARY KEY,
    started_at_ms INTEGER NOT NULL,
    -- NULL tant que la session est ouverte.
    ended_at_ms   INTEGER,
    -- Dernier instant où un live a été vu (sert à fermer la session après un silence).
    last_live_ms  INTEGER NOT NULL,
    peak_viewers  INTEGER NOT NULL DEFAULT 0
);
CREATE INDEX idx_sessions_started ON sessions(started_at_ms);

-- Un échantillon par chaîne en direct à chaque sondage.
CREATE TABLE viewer_samples (
    session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
    ts_ms      INTEGER NOT NULL,
    channel_id TEXT NOT NULL,
    platform   TEXT NOT NULL,
    viewers    INTEGER NOT NULL
);
CREATE INDEX idx_viewer_samples_session ON viewer_samples(session_id, ts_ms);

-- Événements normalisés (follow, sub, gift, raid, don, bits). `session_id` reste
-- NULL pour un événement survenu hors session.
CREATE TABLE session_events (
    seq          INTEGER PRIMARY KEY AUTOINCREMENT,
    session_id   TEXT REFERENCES sessions(id) ON DELETE CASCADE,
    event_id     TEXT NOT NULL DEFAULT '',
    channel_id   TEXT NOT NULL,
    platform     TEXT NOT NULL,
    kind         TEXT NOT NULL,
    who          TEXT NOT NULL,
    amount       INTEGER NOT NULL DEFAULT 0,
    message      TEXT,
    -- StreamEvent complet en JSON, pour le relire tel quel.
    payload      TEXT NOT NULL,
    timestamp_ms INTEGER NOT NULL
);
CREATE INDEX idx_session_events_session ON session_events(session_id, timestamp_ms);
CREATE UNIQUE INDEX idx_session_events_dedupe ON session_events(channel_id, event_id) WHERE event_id <> '';

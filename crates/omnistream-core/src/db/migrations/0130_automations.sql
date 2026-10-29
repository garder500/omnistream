-- Automations locales : règles « si X alors Y » et commandes de chat !cmd.
CREATE TABLE IF NOT EXISTS automation_rules (
    id            TEXT PRIMARY KEY,
    name          TEXT NOT NULL,
    enabled       INTEGER NOT NULL DEFAULT 1,
    trigger_json  TEXT NOT NULL,
    action_json   TEXT NOT NULL,
    platforms     TEXT NOT NULL DEFAULT '[]',
    cooldown_secs INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS chat_commands (
    id                 TEXT PRIMARY KEY,
    name               TEXT NOT NULL UNIQUE COLLATE NOCASE,
    response           TEXT NOT NULL,
    enabled            INTEGER NOT NULL DEFAULT 1,
    cooldown_secs      INTEGER NOT NULL DEFAULT 0,
    user_cooldown_secs INTEGER NOT NULL DEFAULT 0,
    min_role           TEXT NOT NULL DEFAULT 'everyone',
    platforms          TEXT NOT NULL DEFAULT '[]',
    reply_json         TEXT NOT NULL DEFAULT '{"kind":"source"}'
);

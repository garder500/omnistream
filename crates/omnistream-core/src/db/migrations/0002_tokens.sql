ALTER TABLE accounts ADD COLUMN login TEXT NOT NULL DEFAULT '';

CREATE TABLE account_tokens (
    account_id    TEXT PRIMARY KEY REFERENCES accounts(id) ON DELETE CASCADE,
    access_token  TEXT NOT NULL,
    refresh_token TEXT,
    expires_at    INTEGER
);

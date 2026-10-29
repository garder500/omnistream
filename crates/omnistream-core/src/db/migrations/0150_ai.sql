-- Unité IA (BYOK) : historique des résumés générés. La clé API et les réglages
-- vivent dans `settings` (clés `ai_*`) ; rien n'est activé par défaut.
CREATE TABLE ai_summaries (
    id         INTEGER PRIMARY KEY AUTOINCREMENT,
    kind       TEXT NOT NULL,          -- 'live' | 'recap'
    model      TEXT NOT NULL,
    text       TEXT NOT NULL,
    created_at INTEGER NOT NULL        -- ms depuis l'epoch Unix
);

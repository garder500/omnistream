-- Règles d'automod unifié. `kind`, `action` et `platforms` sont des JSON (voir automod.rs).
CREATE TABLE automod_rules (
    id        TEXT PRIMARY KEY,
    name      TEXT NOT NULL,
    enabled   INTEGER NOT NULL DEFAULT 1,
    score     INTEGER NOT NULL DEFAULT 10,
    kind      TEXT NOT NULL,
    action    TEXT NOT NULL,
    platforms TEXT NOT NULL DEFAULT '[]'
);

-- Règles par défaut : l'ancienne heuristique (liens, majuscules, répétitions) + spam inter-plateformes.
INSERT INTO automod_rules (id, name, enabled, score, kind, action, platforms) VALUES
  ('default-links', 'Lien suspect', 1, 20, '{"type":"links","allowed_domains":[]}', '{"type":"flag"}', '[]'),
  ('default-caps', 'Majuscules', 1, 10, '{"type":"caps","min_letters":12,"ratio_percent":80}', '{"type":"flag"}', '[]'),
  ('default-repeat', 'Répétition', 1, 10, '{"type":"repeat","min_words":4,"char_run":15}', '{"type":"flag"}', '[]'),
  ('default-cross-spam', 'Spam inter-plateformes', 1, 40, '{"type":"cross_platform_spam","window_secs":60,"min_channels":2}', '{"type":"flag"}', '[]');

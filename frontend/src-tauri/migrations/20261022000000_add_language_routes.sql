-- Per-language model routing.
-- One row per language code ("he", "en", ...). Each column may be NULL, in
-- which case the global setting applies. summary_mode is "direct" (write the
-- summary in that language) or "translate" (upstream behaviour: English first,
-- then translate).
CREATE TABLE IF NOT EXISTS language_routes (
    language TEXT PRIMARY KEY,
    stt_provider TEXT,
    stt_model TEXT,
    summary_provider TEXT,
    summary_model TEXT,
    summary_mode TEXT NOT NULL DEFAULT 'direct',
    enabled INTEGER NOT NULL DEFAULT 1,
    updated_at TEXT NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%fZ', 'now'))
);

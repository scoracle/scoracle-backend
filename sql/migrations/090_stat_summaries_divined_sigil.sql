-- Migration 090: add divined_sigil to stat_summaries
--
-- local model now divines the entity's sigil on the first output line ("SIGIL: <label>")
-- rather than receiving it as a pre-supplied hint. The divined label is stored here
-- so the SigilCard can surface it as the hero (overriding the pre-computed label).
-- Nullable: rows generated before s3 and marker rows (no local model call) leave it NULL.

ALTER TABLE stat_summaries ADD COLUMN IF NOT EXISTS divined_sigil TEXT;

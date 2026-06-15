-- 086_stat_summaries.sql
--
-- Stat-commentary feature — schema. The STATS-rail twin of news_summaries (081/085):
-- the symmetric other half of the two-rail narrative engine. Where the news rail
-- narrates the scrubbed NEWS corpus, the stats rail narrates the scrubbed DATA — the
-- rating engine is the deterministic "scrub", and Gemma derives the entity's ON-FIELD
-- IDENTITY from the engine's already-curated datapoints (rating_breakdown, composite,
-- specialist, scoped percentiles), NEVER raw box scores.
--
-- The reveal is an ACTUAL ANALYSIS (a few sentences), not a strengths/weaknesses list —
-- that's the differentiator. Framing: our COMPOSITE shows how WELL an entity performs,
-- our SPECIAL shows HOW it performs; the commentary narrates the "how" (the identity),
-- with the composite setting the quality register. Lives in the Special card (players
-- AND teams).
--
-- Clones the vibe_scores (007) / news_summaries (081) entity-grain pattern: append model
-- (a new row each generation; reads take latest-per-entity via DISTINCT ON), which also
-- gives the time-scope `as_of` rail off generated_at for free. UNLIKE news (N narratives
-- per generation) this is ONE row per generation — an entity has a single identity.
--
-- `notability` is the stats-rail analog of news `impact` / transfers `heat`: a
-- DETERMINISTIC 0-100 score of how DISTINCTIVE the entity's profile is (computed from the
-- engine's percentiles — extremity, spread, count of elite buckets — Gemma never invents
-- it), stored with transparent components. It drives the DYNAMIC length (a distinctive
-- specialist earns a fuller analysis; an average profile a terse line) and can rank a
-- "most distinctive identities" board. NULL body/notability = insufficient stats this
-- cycle (the no-corpus marker analog).
--
-- Entity references use the composite (entity_id, sport) pattern used everywhere else.

BEGIN;

CREATE TABLE IF NOT EXISTS stat_summaries (
    id               BIGSERIAL   PRIMARY KEY,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id        INTEGER     NOT NULL,
    sport            TEXT        NOT NULL REFERENCES sports(id),

    -- What kicked off this generation. 'stat_change' = the percentile_changed
    -- listener (the stats analog of a news spike); plus the batch + manual paths.
    trigger_type     TEXT        NOT NULL CHECK (trigger_type IN ('stat_change', 'periodic', 'manual')),
    trigger_payload  JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Gemma output: the on-field identity analysis (original prose).
    -- NULL = insufficient/uninteresting stats this cycle (marker row).
    body             TEXT,

    -- Deterministic distinctiveness (0-100) + transparent components
    -- (top_percentile, spread, elite_count, …). Drives dynamic length + the board.
    notability       SMALLINT    CHECK (notability IS NULL OR notability BETWEEN 0 AND 100),
    notability_components JSONB   NOT NULL DEFAULT '{}'::jsonb,

    -- The scrubbed datapoints fed to Gemma as FACTS (composite, special, the salient
    -- scoped percentiles) — provenance + the grounding trace that keeps the prose honest.
    input_components JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Versioning (nullable — marker rows have no model).
    model_version    TEXT,
    prompt_version   TEXT,

    generated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Latest-per-entity (DISTINCT ON read) + the `as_of` time-scope lookup
-- (generated_at <= :as_of ORDER BY generated_at DESC) + the regenerate debounce.
CREATE INDEX IF NOT EXISTS idx_stat_summaries_entity_recent
    ON stat_summaries(entity_type, entity_id, sport, generated_at DESC);

-- "Most distinctive identities" board: a sport's most notable profiles, newest first.
-- Only rows with a real analysis + notability qualify.
CREATE INDEX IF NOT EXISTS idx_stat_summaries_sport_notability
    ON stat_summaries(sport, notability DESC, generated_at DESC)
    WHERE body IS NOT NULL AND notability IS NOT NULL;

COMMENT ON TABLE stat_summaries IS
    'Append, one row per generation per entity (latest-per-entity read). The STATS-rail narrative: Gemma''s on-field IDENTITY analysis derived from the rating engine''s scrubbed datapoints (composite=how well, special=how). notability (deterministic, distinctiveness) drives dynamic length + a board; NULL body = insufficient-stats marker. Twin of news_summaries; written by ml/stat_commentary.go.';

COMMIT;

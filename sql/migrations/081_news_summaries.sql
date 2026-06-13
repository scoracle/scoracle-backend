-- 081_news_summaries.sql
--
-- News-summary feature — schema. Clones the vibe_scores (007) / transfer_rumors
-- (031) pattern at the ENTITY grain: one Gemma-written news summary per entity
-- per generation. Append model (a new row each run; reads take latest-per-entity
-- via DISTINCT ON), which also gives us the time-scope rail ("summary as it stood
-- N weeks ago") for free off `generated_at`.
--
-- Written by the unified per-entity analysis (Stage-2a) that ALSO writes the
-- sentiment row to vibe_scores from the same Gemma call — sentiment stays in
-- vibe_scores (feeds the Trends sparkline, unchanged); the prose summary lives
-- here. `impact` is the "how big is this news cycle" score (the news analog of
-- transfers' heat) that ranks the news leaderboard; like heat it is meant to be
-- DETERMINISTIC + transparent (computed from the corpus — article count, source
-- tier, recency, velocity — Gemma never invents the number), stored with its
-- components. Nullable everywhere: NULL summary/impact = no corpus this cycle
-- (the persistNoCorpus analog).
--
-- Entity references use the composite (entity_id, sport) pattern used everywhere
-- else. No explicit entity FK — entity_type branches the lookup.

BEGIN;

CREATE TABLE IF NOT EXISTS news_summaries (
    id               BIGSERIAL   PRIMARY KEY,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id        INTEGER     NOT NULL,
    sport            TEXT        NOT NULL REFERENCES sports(id),

    -- What kicked off this generation (mirrors transfer_rumors' CHECK set).
    trigger_type     TEXT        NOT NULL CHECK (trigger_type IN ('news_spike', 'periodic', 'manual')),
    trigger_payload  JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Gemma output. summary NULL = no relevant corpus this cycle.
    summary          TEXT,
    trending_topics  JSONB       NOT NULL DEFAULT '[]'::jsonb,

    -- Deterministic impact (0-100) + transparent components (distinct_sources,
    -- velocity, recency, tier_weight, …). Ranks the news leaderboard.
    impact           SMALLINT    CHECK (impact IS NULL OR impact BETWEEN 0 AND 100),
    impact_components JSONB      NOT NULL DEFAULT '{}'::jsonb,

    source_attribution TEXT,

    -- Traceability into the corpus that informed this row.
    input_news_ids   BIGINT[]    NOT NULL DEFAULT '{}',

    -- Versioning (nullable — no-corpus rows have no model).
    model_version    TEXT,
    prompt_version   TEXT,

    generated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Latest-per-entity (DISTINCT ON read) + the time-scope `as_of` lookup
-- (generated_at <= :as_of ORDER BY generated_at DESC) + the spike debounce.
CREATE INDEX IF NOT EXISTS idx_news_summaries_entity_recent
    ON news_summaries(entity_type, entity_id, sport, generated_at DESC);

-- News leaderboard: a sport's hottest summaries, newest first. Only rows with a
-- real summary + impact qualify.
CREATE INDEX IF NOT EXISTS idx_news_summaries_sport_impact
    ON news_summaries(sport, impact DESC, generated_at DESC)
    WHERE summary IS NOT NULL AND impact IS NOT NULL;

COMMIT;

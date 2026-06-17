-- Migration 091: vibe_synthesis table — the holistic three-pillar vibe score
--
-- Replaces the single-source sentiment_scores as the vibe the product shows.
-- Three pillars: news narrative (transfers-informed), stats Sigil (the divined
-- statistical identity), and momentum (sentiment trend + composite trend).
-- Generated event-driven + debounced, not on a schedule.
--
-- entity_vibes / vibes_leaderboard in db.go are repointed to this table in Phase B.3.

CREATE TABLE vibe_synthesis (
    id               BIGSERIAL   PRIMARY KEY,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('player','team')),
    entity_id        INTEGER     NOT NULL,
    sport            TEXT        NOT NULL REFERENCES sports(id),
    season           INTEGER,
    trigger_type     TEXT        NOT NULL CHECK (trigger_type IN (
                         'narrative_change','sentiment_break','composite_shift',
                         'lazy_view','periodic','manual')),
    trigger_payload  JSONB       NOT NULL DEFAULT '{}'::jsonb,
    score            SMALLINT    CHECK (score IS NULL OR score BETWEEN 1 AND 100),
    previous_score   SMALLINT    CHECK (previous_score IS NULL OR previous_score BETWEEN 1 AND 100),
    blurb            TEXT,
    input_components JSONB       NOT NULL DEFAULT '{}'::jsonb,
    input_hash       TEXT,
    model_version    TEXT,
    prompt_version   TEXT,
    generated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Primary read: latest per entity
CREATE INDEX idx_vibe_synthesis_entity_recent
    ON vibe_synthesis(entity_type, entity_id, sport, generated_at DESC);

-- Leaderboard: scored + blurbed rows by sport
CREATE INDEX idx_vibe_synthesis_sport_score
    ON vibe_synthesis(sport, score DESC, generated_at DESC)
    WHERE score IS NOT NULL AND blurb IS NOT NULL;

-- Vibe blurb storage — ~140 char narrative summaries produced by Gemma.
--
-- Not a numerical rating. The blurb is qualitative, news/tweet-driven,
-- with the milestone fact sprinkled in lightly.
--
-- Entity references use the composite (entity_id, sport) pattern we use
-- everywhere else. No explicit FK — entity_type branches the lookup.

CREATE TABLE IF NOT EXISTS vibe_scores (
    id               BIGSERIAL PRIMARY KEY,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id        INTEGER     NOT NULL,
    sport            TEXT        NOT NULL REFERENCES sports(id),

    -- What kicked off this vibe generation.
    trigger_type     TEXT        NOT NULL CHECK (trigger_type IN ('milestone', 'manual', 'periodic')),
    trigger_payload  JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- The Gemma output.
    blurb            TEXT        NOT NULL,

    -- Traceability: which corpus rows informed this specific blurb.
    -- Useful for "why did Gemma say that" debugging and for cost/quality
    -- analysis when we iterate prompts.
    input_news_ids   BIGINT[]    NOT NULL DEFAULT '{}',
    input_tweet_ids  TEXT[]      NOT NULL DEFAULT '{}',

    -- Versioning: store the model + prompt version so we can A/B prompts
    -- or backfill when either changes. prompt_version is a short label
    -- like "v1" defined in the vibe package.
    model_version    TEXT        NOT NULL,
    prompt_version   TEXT        NOT NULL,

    generated_at     TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Most common read: "give me the latest blurb(s) for this entity."
CREATE INDEX IF NOT EXISTS idx_vibe_scores_entity_recent
    ON vibe_scores(entity_type, entity_id, sport, generated_at DESC);

-- Cross-sport "recent blurbs" feed.
CREATE INDEX IF NOT EXISTS idx_vibe_scores_recent
    ON vibe_scores(generated_at DESC);

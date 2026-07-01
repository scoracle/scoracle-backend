-- 031_transfer_rumors.sql
--
-- Transfers/Trades feature — schema. Clones the vibe_scores pattern (007) at the
-- PAIR grain: one rumor is a (team, player) link mined from the news co-mention
-- graph (news_article_entities), scored with a DETERMINISTIC heat index, and
-- VETTED by local model (the local model fields are nullable — NULL = not-yet-vetted / no
-- rumor / cleared, the persistNoCorpus analog). Append model like vibe_scores:
-- a new row per generation; reads take latest-per-pair via DISTINCT ON.
--
-- Heat is computed in SQL (migration 032), stored with its components for
-- transparency (like rating_breakdown, 030). local model never invents the number.

BEGIN;

-- ---------------------------------------------------------------------------
-- source_tiers — credibility weighting for the heat index. Covers BOTH news
-- publications (news_articles.source) and tweet author handles
-- (tweets.author_username) — the tier-1 transfer sources (Romano, Ornstein,
-- Woj/Shams/Schefter) break on Twitter and never appear as a news source, while
-- the news side is dominated by aggregator churn. Only KNOWN-good sources get
-- rows; the heat SQL defaults unknowns to a low weight (COALESCE(weight, 0.3)),
-- so the aggregator long tail is discounted by design.
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS source_tiers (
    source     TEXT         NOT NULL,
    kind       TEXT         NOT NULL CHECK (kind IN ('news', 'twitter')),
    tier       SMALLINT     NOT NULL CHECK (tier BETWEEN 1 AND 3),
    weight     NUMERIC(4,2) NOT NULL,
    updated_at TIMESTAMPTZ  NOT NULL DEFAULT NOW(),
    PRIMARY KEY (kind, source)
);

-- Seed (starter set — refine freely). Matched case-insensitively in the heat SQL.
INSERT INTO source_tiers (source, kind, tier, weight) VALUES
    -- Tier-1 news publications.
    ('The Athletic',       'news', 1, 1.00),
    ('BBC',                'news', 1, 1.00),
    ('BBC Sport',          'news', 1, 1.00),
    ('Sky Sports',         'news', 1, 1.00),
    ('ESPN',               'news', 1, 1.00),
    ('The New York Times', 'news', 1, 1.00),
    ('The Guardian',       'news', 1, 0.90),
    -- Tier-2 major outlets.
    ('CBS Sports',         'news', 2, 0.60),
    ('Sports Illustrated', 'news', 2, 0.60),
    ('The Telegraph',      'news', 2, 0.60),
    ('Goal.com',           'news', 2, 0.60),
    ('Sporting News',      'news', 2, 0.50),
    ('ClutchPoints',       'news', 2, 0.50),
    -- Tier-1 transfer/trade journalists (Twitter handle = tweets.author_username, no @).
    ('FabrizioRomano',     'twitter', 1, 1.00),
    ('David_Ornstein',     'twitter', 1, 1.00),
    ('wojespn',            'twitter', 1, 1.00),
    ('ShamsCharania',      'twitter', 1, 1.00),
    ('AdamSchefter',       'twitter', 1, 1.00),
    ('RapSheet',           'twitter', 1, 1.00),
    ('TheAthleticFC',      'twitter', 2, 0.70)
ON CONFLICT (kind, source) DO NOTHING;

-- ---------------------------------------------------------------------------
-- transfer_rumors — one scored (team, player) rumor per generation (append).
-- ---------------------------------------------------------------------------
CREATE TABLE IF NOT EXISTS transfer_rumors (
    id                 BIGSERIAL   PRIMARY KEY,
    team_id            INTEGER     NOT NULL,
    player_id          INTEGER     NOT NULL,
    sport              TEXT        NOT NULL REFERENCES sports(id),

    -- What kicked off this generation. news_spike is in the CHECK from day one
    -- (vibe_scores omitted its 'news_spike' — a latent gap; not repeated here).
    trigger_type       TEXT        NOT NULL CHECK (trigger_type IN ('news_spike', 'periodic', 'manual')),
    trigger_payload    JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- Deterministic heat (0-100) + its transparent components (distinct_sources,
    -- velocity, recency, tier_weight, …) — drawn from the pair corpus.
    heat               SMALLINT    CHECK (heat IS NULL OR heat BETWEEN 0 AND 100),
    heat_components    JSONB       NOT NULL DEFAULT '{}'::jsonb,

    -- local model vetting — all nullable. NULL = not vetted yet / no corpus / cleared.
    is_rumor           BOOLEAN,
    direction          TEXT        CHECK (direction IS NULL OR direction IN ('incoming', 'outgoing', 'unclear')),
    stage              TEXT        CHECK (stage IS NULL OR stage IN ('speculation', 'concrete_interest', 'advanced_talks', 'here_we_go')),
    model_summary      TEXT,
    source_attribution TEXT,
    confidence         NUMERIC(3,2),

    -- Traceability into the corpus that informed this row.
    input_news_ids     BIGINT[]    NOT NULL DEFAULT '{}',
    input_tweet_ids    TEXT[]      NOT NULL DEFAULT '{}',

    -- Versioning (nullable — heat-only rows have no model; set on local model vet).
    model_version      TEXT,
    prompt_version     TEXT,

    generated_at       TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- v1 team-card read: a team's live rumors by heat, newest first.
CREATE INDEX IF NOT EXISTS idx_transfer_rumors_team_heat
    ON transfer_rumors(team_id, sport, heat DESC, generated_at DESC) WHERE is_rumor IS TRUE;

-- Latest-per-pair (DISTINCT ON read) + the debounce lookup.
CREATE INDEX IF NOT EXISTS idx_transfer_rumors_pair_recent
    ON transfer_rumors(team_id, player_id, sport, generated_at DESC);

-- Deferred player-suitors view.
CREATE INDEX IF NOT EXISTS idx_transfer_rumors_player
    ON transfer_rumors(player_id, sport, generated_at DESC);

COMMIT;

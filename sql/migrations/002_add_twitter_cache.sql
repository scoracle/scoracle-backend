-- Lazy cache for X (Twitter) journalist lists, one row per sport.
-- See progress_docs/2026-04-15_twitter-lazy-cache.md for architecture.

-- Per-sport list registry. Fetched on demand when last_fetched_at is older than ttl_seconds.
CREATE TABLE IF NOT EXISTS twitter_lists (
    sport             TEXT PRIMARY KEY,
    list_id           TEXT NOT NULL,
    ttl_seconds       INT  NOT NULL DEFAULT 1200,
    since_id          TEXT,
    last_fetched_at   TIMESTAMPTZ,
    last_error        TEXT,
    last_error_at     TIMESTAMPTZ,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- Cached tweets. Short-lived per ToS; purged by TTL worker.
CREATE TABLE IF NOT EXISTS tweets (
    id                        TEXT PRIMARY KEY,
    sport                     TEXT NOT NULL REFERENCES twitter_lists(sport) ON DELETE CASCADE,
    author_id                 TEXT NOT NULL,
    author_username           TEXT NOT NULL,
    author_name               TEXT NOT NULL,
    author_verified           BOOLEAN NOT NULL DEFAULT FALSE,
    author_profile_image_url  TEXT,
    text                      TEXT NOT NULL,
    posted_at                 TIMESTAMPTZ NOT NULL,
    likes                     INT NOT NULL DEFAULT 0,
    retweets                  INT NOT NULL DEFAULT 0,
    replies                   INT NOT NULL DEFAULT 0,
    fetched_at                TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_tweets_sport_posted_at
    ON tweets (sport, posted_at DESC);

-- Entity link table (many-to-many between tweets and sport entities).
-- entity_type is 'player' | 'team'. entity_id references the sport-scoped entity.
CREATE TABLE IF NOT EXISTS tweet_entities (
    tweet_id     TEXT NOT NULL REFERENCES tweets(id) ON DELETE CASCADE,
    sport        TEXT NOT NULL,
    entity_type  TEXT NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id    INT  NOT NULL,
    PRIMARY KEY (tweet_id, sport, entity_type, entity_id)
);

CREATE INDEX IF NOT EXISTS idx_tweet_entities_lookup
    ON tweet_entities (sport, entity_type, entity_id);

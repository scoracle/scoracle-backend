-- News persistence for Gemma 4 context + training corpus.
--
-- news_articles holds the raw article metadata (title + description + source).
-- We do NOT scrape article bodies — RSS snippets are sufficient per the
-- product decision (keeps pipeline simple, respects publisher TOS).
--
-- news_article_entities links each article to matched player/team entities.
-- ON DELETE CASCADE so article purges don't leave orphan links (though we
-- currently never purge; corpus grows indefinitely).

CREATE TABLE IF NOT EXISTS news_articles (
    id              BIGSERIAL PRIMARY KEY,
    url_hash        TEXT        NOT NULL UNIQUE,  -- SHA-256 of normalized URL; dedupes re-fetches
    url             TEXT        NOT NULL,
    source          TEXT,                         -- "ESPN", "Bleacher Report", etc.
    title           TEXT        NOT NULL,
    description     TEXT,                         -- RSS snippet, ~300 chars
    published_at    TIMESTAMPTZ,
    fetched_at      TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    raw             JSONB       NOT NULL DEFAULT '{}'::jsonb
);

CREATE INDEX IF NOT EXISTS idx_news_articles_published_at
    ON news_articles(published_at DESC);

CREATE INDEX IF NOT EXISTS idx_news_articles_fetched_at
    ON news_articles(fetched_at DESC);


CREATE TABLE IF NOT EXISTS news_article_entities (
    article_id       BIGINT      NOT NULL REFERENCES news_articles(id) ON DELETE CASCADE,
    entity_type      TEXT        NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id        INTEGER     NOT NULL,
    sport            TEXT        NOT NULL REFERENCES sports(id),
    match_confidence NUMERIC(4,3),                -- 0.000-1.000 from the matcher
    created_at       TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (article_id, entity_type, entity_id, sport)
);

CREATE INDEX IF NOT EXISTS idx_news_entities_lookup
    ON news_article_entities(entity_type, entity_id, sport);

CREATE INDEX IF NOT EXISTS idx_news_entities_by_article
    ON news_article_entities(article_id);

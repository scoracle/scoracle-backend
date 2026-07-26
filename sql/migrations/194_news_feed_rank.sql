-- 194: capture Google's own ranking, and use it to spend a finite reading budget.
--
-- The Article Reader is the sole relevance judge and the only stage that reads the publisher
-- body, but it cannot read everything: ingest admits ~6,700 articles/day and the Reader sustains
-- ~800/day sharing one GPU with eight other stages. Until now the queue was drained FIFO, so the
-- articles that got a model call were simply the ones that arrived first.
--
-- Google already ranks its results — the RSS payload comes back ordered, best first. That ordering
-- was being thrown away twice: nothing captured it at parse time, and sortArticlesByDate re-sorted
-- the slice by date before persist. This column carries it.
--
-- NULL means "ingested before this column existed" and must sort last, never first: an unranked
-- backlog article should not outrank a fresh Google top hit.
ALTER TABLE public.news_articles
    ADD COLUMN IF NOT EXISTS feed_rank integer;

COMMENT ON COLUMN public.news_articles.feed_rank IS
    'Best (lowest) 0-based position Google ever returned this article at, across every entity query '
    'that surfaced it. Ingest keeps the minimum via LEAST() on conflict: one article is returned by '
    'several teams'' queries at different positions, so there is no single global rank — but "the '
    'highest Google ever ranked it, for anyone" is a fact, and it is the one that decides whether the '
    'article is worth a model call. NULL = pre-migration backlog; sorts last.';

-- The Article Reader claims by rank; the partial index matches that claim exactly. Only unread
-- articles are ever ordered this way, so the index stays small as the corpus grows.
CREATE INDEX IF NOT EXISTS idx_news_articles_feed_rank
    ON public.news_articles (feed_rank NULLS LAST, id)
    WHERE duplicate_of IS NULL;

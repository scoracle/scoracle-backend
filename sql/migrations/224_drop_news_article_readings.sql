-- 224_drop_news_article_readings.sql
--
-- The last legacy-rail table goes. `news_article_readings` was the two-rail Editor's
-- read ledger; the one-rail Editor has written `editor_reads` since the cutover, and
-- the legacy table's newest row is 2026-08-05. Its two remaining readers were both
-- soft: the graph junction's COALESCE fallback (the G1 seam, removed in the same
-- deploy) and `collapse_exact_title_duplicates`, which used it only as a canonical-pick
-- tiebreak.
--
-- The freshness watchdog was ALSO still pointed here, which is why editor_reads[*]
-- alarmed 0/N on a healthy pipeline from Aug 15 — the one-rail Editor read 1,539
-- articles in the 24h before this migration, all invisible to a check that joined the
-- dead table. The watchdog repoint ships with this migration.
--
-- Deploy order (F-022 — this is a table DROP): release the binaries that stop reading
-- the table FIRST (graph seam removal, remap binary deleted), then apply.

BEGIN;

-- 1. The dedup tiebreak repoints to the one-rail read ledger. Same shape as prod
--    (post-214: no vetted predicate on news_article_entities); only the
--    `already_read` EXISTS moves from news_article_readings to editor_reads.
CREATE OR REPLACE FUNCTION public.collapse_exact_title_duplicates(
    p_lookback       interval DEFAULT interval '72 hours',
    p_min_title_len  integer  DEFAULT 30
) RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    v_marked integer;
BEGIN
    WITH cand AS (
        SELECT a.id,
               a.source,
               a.published_at,
               a.feed_rank,
               -- strip punctuation, fold accents, collapse runs of spaces
               unaccent(lower(regexp_replace(
                   regexp_replace(a.title, '[^a-zA-Z0-9 ]', '', 'g'), ' +', ' ', 'g'))) AS norm,
               EXISTS (SELECT 1 FROM public.news_article_entities e
                        WHERE e.article_id = a.id) AS corpus_visible,
               EXISTS (SELECT 1 FROM public.editor_reads er
                        WHERE er.article_id = a.id) AS already_read
          FROM public.news_articles a
         WHERE a.published_at > now() - p_lookback
           AND a.title <> ''
           AND a.duplicate_of IS NULL
    ),
    grp AS (
        SELECT norm
          FROM cand
         WHERE length(norm) >= p_min_title_len
         GROUP BY norm
        HAVING count(*) > 1
           AND count(DISTINCT source) > 1     -- cross-source only
    ),
    ranked AS (
        SELECT c.id,
               c.source,
               first_value(c.id) OVER w     AS canonical_id,
               first_value(c.source) OVER w AS canonical_source
          FROM cand c
          JOIN grp g ON g.norm = c.norm
        WINDOW w AS (
            PARTITION BY c.norm
            ORDER BY c.corpus_visible DESC,
                     c.already_read    DESC,
                     c.published_at    ASC,
                     c.feed_rank       ASC NULLS LAST,
                     c.id              ASC
        )
    )
    UPDATE public.news_articles a
       SET duplicate_of = r.canonical_id
      FROM ranked r
     WHERE a.id = r.id
       AND r.id <> r.canonical_id
       -- Per-PAIR cross-source check, not per-group. A group of {A, A, B} passes the group-level
       -- `count(DISTINCT source) > 1` test, and without this the second A would be suppressed by
       -- its own sibling -- exactly the same-source collapse the deleted cosine branch was doing.
       -- The second A stays canonical; only B's copy is suppressed.
       AND r.source IS DISTINCT FROM r.canonical_source
       AND a.duplicate_of IS NULL;

    GET DIAGNOSTICS v_marked = ROW_COUNT;
    RETURN v_marked;
END;
$$;

-- 2. The table itself.
DROP TABLE public.news_article_readings;

INSERT INTO public.schema_migrations(version) VALUES ('224_drop_news_article_readings')
    ON CONFLICT DO NOTHING;

COMMIT;

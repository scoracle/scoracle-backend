-- Preserve cross-source exact-title dedup after Editor retirement.
-- The canonical choice favors acquired publisher text, then the oldest copy.
BEGIN;
CREATE FUNCTION public.harvester_collapse_exact_title_duplicates(p_lookback interval DEFAULT '72:00:00'::interval, p_min_title_len integer DEFAULT 30) RETURNS integer
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
               EXISTS (SELECT 1 FROM public.harvester_acquisitions ha
                        WHERE ha.article_id = a.id AND ha.status = 'acquired') AS already_acquired
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
                     c.already_acquired    DESC,
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
COMMENT ON FUNCTION public.harvester_collapse_exact_title_duplicates(interval, integer) IS
    'Collapse byte-identical cross-source headlines; prefer corpus-visible or Harvester-acquired publisher text, then oldest. No Editor read dependency.';
INSERT INTO public.schema_migrations(version) VALUES ('270_harvester_dedup_owner') ON CONFLICT DO NOTHING;
COMMIT;

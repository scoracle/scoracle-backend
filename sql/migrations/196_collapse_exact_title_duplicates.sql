-- 196_collapse_exact_title_duplicates.sql
--
-- WHAT: a sweep that points byte-identical CROSS-SOURCE articles at one canonical copy, closing a
-- race the per-article novelty gate cannot close on its own.
--
-- WHY: `novelty::gate` suppresses near-verbatim reposts at scrub time (token-Jaccard >= 0.90, 72h
-- lookback) by comparing an article against recent CANONICAL coverage of its OWN VETTED ENTITIES.
-- Two copies scrubbed in the same pass, before either has vetted membership, are invisible to each
-- other -- neither is in the other's candidate set. Measured 2026-07-28 over a 14-day window:
-- 2,057 of 32,016 corpus-visible articles (6.4%) are exact-title duplicates of another
-- corpus-visible article. That is 6.4% of every busyness verdict counting one story twice.
--
-- This is a sweep, not a second gate. It runs after the fact, which is the only thing that can fix
-- a race.
--
-- THREE GUARDS, each earned:
--
--   * CROSS-SOURCE ONLY. The novelty gate's same-source branch was deliberately deleted (teardown
--     plan section 2.2): it suppressed on cosine alone and collapsed two opposite takes on Cade
--     Cunningham into one. Scott's call, recorded there: "a publication posting twice about the same
--     thing is signal about that publication, not grounds to throw the second one out." Measured
--     here, 1,122 of 3,467 duplicate-title groups are same-source. They stay.
--
--   * MINIMUM TITLE LENGTH. Section headers repeat verbatim across unrelated articles -- measured
--     in this corpus: "Watch", "El Tiempo" (10 copies), "Match Centre", "Off the wire",
--     "Game Changers". Every observed false grouping was under 30 normalized characters, and the
--     guard costs only ~68 of 1,799 collapsible articles. Nearly free insurance.
--
--   * CANONICAL PREFERS THE CORPUS-VISIBLE COPY. Choosing a non-visible article as canonical would
--     REMOVE a visible one from the corpus -- the "permanent hole in the narrative" the novelty
--     gate's doc names as the failure that matters. Order: corpus-visible first, then already-read
--     (never waste a paid model call), then oldest (FIRST-SEEN WINS, matching the gate), then best
--     feed_rank, then lowest id as a deterministic tiebreak.
--
-- Writes `duplicate_of` ONLY -- never `news_article_entities.vetted` -- so membership is untouched
-- and no derive re-fires. Same discipline as `novelty::mark_duplicate`.
--
-- Cannot create chains: only articles with `duplicate_of IS NULL` are considered, and the canonical
-- of each group keeps NULL. Idempotent -- re-running is a no-op once a group is collapsed.
--
-- Deploy order: additive (a new function, called by nothing until the binary ships).

BEGIN;

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
                        WHERE e.article_id = a.id AND e.vetted IS TRUE) AS corpus_visible,
               EXISTS (SELECT 1 FROM public.news_article_readings r
                        WHERE r.article_id = a.id) AS already_read
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

COMMENT ON FUNCTION public.collapse_exact_title_duplicates(interval, integer) IS
    'Points byte-identical CROSS-SOURCE articles at one canonical copy, closing the race the '
    'per-article novelty gate cannot: two copies scrubbed before either has vetted membership are '
    'invisible to each other. Same-source repeats are NEVER collapsed (they are signal about the '
    'publication). Titles under p_min_title_len normalized chars are skipped (section headers like '
    '"Match Centre" repeat across unrelated articles). Canonical prefers the corpus-visible copy, '
    'then the already-read one, then the oldest. Writes duplicate_of only; membership untouched.';

INSERT INTO public.schema_migrations(version) VALUES ('196_collapse_exact_title_duplicates')
    ON CONFLICT DO NOTHING;

COMMIT;

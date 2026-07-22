-- 178_source_reliability_for_pair.sql
--
-- Cognition refactor, Phase 4 (The Insider: memory-driven steam + source reliability).
-- A dedicated, ADDITIVE card fn that reports the MEASURED track record of the sources
-- reporting a (player, team) pair's current transfer corpus, so the transfers voice can
-- weigh a claim by WHO is making it (a Sky/Romano-tier source vs a rumour mill). Pure
-- CREATE FUNCTION — the battle-tested narrative_context_for_pair story-memory card is left
-- untouched: "the story's arc" and "the reporters' track record" stay separate SQL
-- primitives, composed at the prompt layer (transfer.rs) exactly like the existing cards.
--
-- WHICH SOURCES: exactly the sources of the pair's LIVE transfer corpus, reusing
-- compute_transfer_heat's news_ids as the single definition of "the pair's corpus" (vetted
-- both sides, title proximity, transfer bucket, 14-day window). No filter duplication, no
-- drift — so the reliability lines annotate precisely the [source] tags the prompt's News
-- headlines already show. The displayed name is the corpus spelling (cs.src) so a card line
-- matches its headline tag verbatim; the join to the record is case-insensitive.
--
-- THE NUMBERS: each source's GLOBAL per-sport record from source_performance (mig 157 family,
-- recomputed nightly by refresh_source_performance in cron-narrative-links.sh):
--   reliability      0-100, n-based belief 100*confirmed/(confirmed+k) (k=5) — the headline
--   confirmed/pairs  the raw base rate (of the pairs the source was attributed on, how many
--                    became applied moves)
--   early_confirmed  how many it reported EARLY, before the pair reached advanced_talks — the
--                    "called it before the pack" steam signal, rendered only when > 0
-- Ordered most-reliable first, capped at 6 so the prompt stays bounded. NULL when no corpus
-- source has a measured record (the card renders nothing).
--
-- Presented, not gatekept (matching the source_performance table doc): the model WEIGHS this,
-- nothing filters by it. Model-facing enrichment only, never user-exposed. Consumed by the
-- transfers prompt builder (t9). ADDITIVE + binary-compatible: nothing calls this fn until the
-- t9 binary lands, so it may be applied before OR after the deploy.

BEGIN;

CREATE OR REPLACE FUNCTION public.source_reliability_for_pair(p_sport text, p_player_id integer, p_team_id integer) RETURNS text
    LANGUAGE sql STABLE
    AS $$
WITH corpus_sources AS (
    -- The distinct sources of the pair's CURRENT transfer corpus. Reuse compute_transfer_heat's
    -- news_ids so "the corpus" has ONE definition and this card can never drift from what the
    -- prompt shows. Empty news_ids (heat NULL / no corpus) ⇒ no rows ⇒ NULL card.
    SELECT DISTINCT a.source AS src
    FROM public.compute_transfer_heat(p_team_id, p_player_id, p_sport) h
    CROSS JOIN LATERAL unnest(h.news_ids) AS n(news_id)
    JOIN public.news_articles a ON a.id = n.news_id
    WHERE a.source IS NOT NULL AND a.source <> ''
),
ranked AS (
    -- Join each corpus source to its measured record; keep the corpus spelling for display so a
    -- card line reads back to its [source] headline tag. Case-insensitive match into the record.
    SELECT cs.src AS source, sp.reliability, sp.confirmed_covered,
           sp.pairs_covered, sp.early_confirmed
    FROM corpus_sources cs
    JOIN public.source_performance sp
      ON sp.sport = p_sport AND lower(sp.source) = lower(cs.src)
    ORDER BY sp.reliability DESC, sp.confirmed_covered DESC, cs.src
    LIMIT 6
)
SELECT NULLIF(string_agg(
    format('%s: reliability %s/100 (%s of %s tracked move%s confirmed%s).',
           r.source, r.reliability, r.confirmed_covered, r.pairs_covered,
           CASE WHEN r.pairs_covered = 1 THEN '' ELSE 's' END,
           CASE WHEN r.early_confirmed > 0
                THEN ', ' || r.early_confirmed || ' reported early' ELSE '' END),
    E'\n' ORDER BY r.reliability DESC, r.confirmed_covered DESC, r.source), '')
FROM ranked r;
$$;

COMMENT ON FUNCTION public.source_reliability_for_pair(p_sport text, p_player_id integer, p_team_id integer) IS
'The measured track record of the sources on one (player, team) pair''s live transfer corpus, as compact prompt lines: for each source shown in the pair''s current News headlines, its GLOBAL per-sport source_performance record — reliability N/100, confirmed/tracked base rate, and early-call count. Sources are the pair''s corpus sources (reuses compute_transfer_heat.news_ids — one corpus definition, no drift); the numbers are each source''s overall record. Ordered most-reliable first, capped at 6. NULL = no corpus source has a measured record. Presented, not gatekept: the transfers voice WEIGHS it for steam vs fizzle, nothing filters by it. Consumed by the transfers prompt builder (t9) — model-facing, never user-facing.';

INSERT INTO public.schema_migrations(version) VALUES ('178_source_reliability_for_pair')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying to prod: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

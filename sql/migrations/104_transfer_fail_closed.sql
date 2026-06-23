-- 104_transfer_fail_closed.sql  (FIRST-GPT-AUDIT Session 10 — make transfer validation fail closed)
--
-- Three changes, all tightening "only a successful POSITIVE Gemma verdict can become
-- a served or downstream-consumed rumor":
--
--   1. Drop the unused Phase-1 fail-OPEN seeder seed_transfer_rumors(). It inserts
--      is_rumor=TRUE straight off the deterministic heat with NO Gemma vet — exactly
--      the fail-open path this session removes from ml/transfer.go. It has no live
--      callers (the Gemma-vetting worker GenerateForTeam replaced it; verified by grep
--      + against the live DB) so it is dropped, not kept around as a foot-gun.
--
--   2. Redefine compute_transfer_heat to require BOTH the team and player link to be
--      vetted IS TRUE. The live def allowed `(vetted IS TRUE OR scrubbed_at IS NULL)`,
--      i.e. it let UNSCRUBBED links contribute heat — heat could exist before Gemma
--      ever saw the link. The candidate selector (loadCandidates) already required
--      vetted IS TRUE on both sides; this makes the heat primitive agree. Audit:
--      "Require both article/entity links to be vetted IS TRUE in compute_transfer_heat."
--
--   3. Drop the vestigial tweet_ids OUT param + the input_tweet_ids columns on
--      transfer_rumors and vibe_scores (simplification C; X was decommissioned in
--      migration 098, both tweet tables already dropped). Confirmed no remaining
--      consumer: only compute_transfer_heat (redefined here) and seed_transfer_rumors
--      (dropped here) referenced them — no views/matviews, and the Go writers
--      (ml/transfer.go, ml/vibe.go) drop the columns from their INSERTs in the same
--      change that ships this migration.
--
-- F-015 note: this was authored against the LIVE function/table definitions (read with
-- pg_get_functiondef / information_schema), not the migration ledger.
--
-- Ordering: drop seed_transfer_rumors first (it depends on both compute_transfer_heat
-- and input_tweet_ids). Then signature-change compute_transfer_heat via DROP+CREATE
-- (CREATE OR REPLACE cannot alter OUT parameters). Then drop the columns.
--
-- DEPLOY ORDER — release the NEW binary FIRST, THEN apply this migration (the
-- reverse of the usual F-001 "migrate before restart"). The new binary is compatible
-- with BOTH schemas: it SELECTs a named subset (heat, components, news_ids) of
-- compute_transfer_heat, which works whether or not the tweet_ids OUT param exists,
-- and it omits input_tweet_ids from its INSERTs, which works whether or not the column
-- exists (it has a DEFAULT '{}'). The OLD binary, by contrast, BREAKS on the new schema
-- (it writes input_tweet_ids and reads the 4th OUT col). So: release.sh → then apply
-- this migration. No new/changed prepared statements ship in this session, so db.New
-- prepares cleanly at boot against either schema.

BEGIN;

-- 1. Remove the unused fail-OPEN heat-only seeder.
DROP FUNCTION IF EXISTS public.seed_transfer_rumors(text, integer);

-- 2. Redefine the heat primitive: vetted-only corpus, no tweet_ids OUT param.
DROP FUNCTION IF EXISTS public.compute_transfer_heat(integer, integer, text);
CREATE FUNCTION public.compute_transfer_heat(
    p_team_id integer, p_player_id integer, p_sport text,
    OUT heat smallint, OUT components jsonb, OUT news_ids bigint[]
)
 RETURNS record
 LANGUAGE sql
 STABLE
AS $function$
    WITH corpus AS (
        -- News articles linking BOTH the team and the player, in PROXIMITY and
        -- Gemma-VETTED on both sides. S10: an unscrubbed link no longer contributes
        -- heat — heat now requires a positive scrub verdict (vetted IS TRUE), so a
        -- rumor's deterministic heat can never exist ahead of its validation.
        SELECT 'news'::text AS kind, a.id::text AS item_id, a.source AS src, a.published_at AS ts
        FROM news_articles a
        JOIN news_article_entities te ON te.article_id = a.id AND te.entity_type='team'
             AND te.entity_id = p_team_id AND te.sport = p_sport
        JOIN news_article_entities pe ON pe.article_id = a.id AND pe.entity_type='player'
             AND pe.entity_id = p_player_id AND pe.sport = p_sport
        WHERE a.published_at > NOW() - INTERVAL '14 days'
          AND te.vetted IS TRUE
          AND pe.vetted IS TRUE
          AND (te.title_pos IS NULL OR pe.title_pos IS NULL
               OR abs(te.title_pos - pe.title_pos) <= 50)
    ),
    agg AS (
        SELECT
            count(DISTINCT c.src) AS distinct_sources,
            count(*) FILTER (WHERE c.ts > NOW() - INTERVAL '3 days') AS recent3,
            count(*) AS total,
            max(c.ts) AS newest,
            COALESCE(MAX(st.weight), 0.3) AS tier_weight
        FROM corpus c
        LEFT JOIN source_tiers st ON lower(st.source) = lower(c.src) AND st.kind = c.kind
    ),
    calc AS (
        SELECT *,
            EXTRACT(EPOCH FROM (NOW() - newest)) / 3600.0 AS age_hours,
            LEAST(1.0, distinct_sources::numeric / 5.0)   AS volume,
            recent3::numeric / GREATEST(total, 1)         AS recent_frac
        FROM agg
    ),
    fin AS (
        SELECT *, exp(-age_hours / 72.0) AS recency FROM calc
    )
    SELECT
        CASE WHEN total = 0 THEN NULL
             ELSE GREATEST(0, LEAST(100,
                    round(100 * tier_weight * recency * (0.6 * volume + 0.4 * recent_frac))))::smallint
        END,
        CASE WHEN total = 0 THEN '{}'::jsonb
             ELSE jsonb_build_object(
                'distinct_sources', distinct_sources,
                'recent_3d', recent3,
                'total_14d', total,
                'newest_age_hours', round(age_hours::numeric, 1),
                'tier_weight', tier_weight,
                'volume', round(volume::numeric, 3),
                'recency', round(recency::numeric, 3),
                'recent_frac', round(recent_frac::numeric, 3))
        END,
        COALESCE((SELECT array_agg(item_id::bigint) FROM corpus WHERE kind='news'), '{}')
    FROM fin;
$function$;

-- 3. Drop the vestigial tweet provenance columns (X decommissioned, migration 098).
ALTER TABLE public.transfer_rumors DROP COLUMN IF EXISTS input_tweet_ids;
ALTER TABLE public.vibe_scores     DROP COLUMN IF EXISTS input_tweet_ids;

COMMIT;

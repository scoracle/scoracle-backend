-- 221_peak_retirement.sql
--
-- WAVE B of the PEAK removal (plan of record: scoracle-wiki/wiki/Plan - PEAK removal.md).
-- Wave A (backend 929511d) took the specialist lens out of the Rust cognition daemon and
-- stopped the model ever seeing the concept. This migration is the storage half: PEAK
-- leaves the schema, and the whole labeling zoo (PEAK + specialist + composite) collapses
-- to one word — **rating** — per §3d.
--
-- WHAT GOES
--   · specialist quartet on player_stats/team_stats/rating_history
--     (rating_specialist, _rank, _score, rating_specialty) and the event projections
--     (rating_specialist, rating_specialist_pct, rating_specialty).
--   · stat_summaries.divined_peak (code-owned since scout s18, never written since s19).
--   · rating_mode_peak_payload() — the back-compat coalescer that renamed specialist keys
--     to peak keys on write — and rating_breakdown_without_specialty(), which stripped a
--     flag that the engine will simply stop emitting.
--   · the specialist indexes.
--
-- WHAT RENAMES (the §3d "uniform rating" pass — one break, not two)
--   rating_composite        → rating
--   rating_composite_rank   → rating_rank
--   rating_composite_score  → rating_score
--   rating_composite_pct    → rating_pct           (event projections)
--   peak_trajectory{,_label,_components} → rating_trajectory{,_label,_components}
--   rating_modes payload keys: composite{,_rank,_score} → rating{,_rank,_score};
--                              peak/peak_rank/peak_score/peak_label dropped.
--   pipeline_work.stage 'peak' → 'rating' (the queue stage was literally named for the
--                              retired concept; input_version prefixes follow in Rust/Go).
--
-- FUNCTIONS REBUILT (all derived from the CURRENT prod definitions via
-- pg_get_functiondef, per sql/README-migrations.md step 3 — the rating engine lives in
-- migrations, not in the canonical sql/*.sql files, so there is nothing to mirror back):
--   _compute_rating_bundle, compute_rating, compute_team_rating, compute_event_starline,
--   compute_team_event_starline, recalculate_event_rating_pct, snapshot_rating_history,
--   mark_momentum_refresh_from_event_rating, refresh_momentum_scores,
--   stat_context_for_entity, narrative_context_for_entity.
--
-- The two memory-card renderers (stat_context_for_entity / narrative_context_for_entity)
-- lose their divined-label line. The prior-season read now carries what survives the
-- retirement: profile distinctiveness plus the (composite-only, dynamic-window) trajectory
-- label. This also retires the Rust-side descrub_memory_card shim.
--
-- BEHAVIOUR NOTE (deliberate, verified): the compute functions inner-joined the specialist
-- CTE, so an entity with no in-spec datapoint could never be rated. Dropping that join
-- widens the rated cohort in principle; measured on live data 2026-08-14 the change is
-- nil — 0 of 42,964 player rows and 0 of 1,176 team rows lack an in-spec datapoint.
--
-- NOT TOUCHED, FOREVER: narrative_episodes.peak_strength/peaked_at/peak_components and
-- storyline_entities.peak_impact are story-coverage peaks — a different concept that
-- shares a word.
--
-- Deploy order — DESTRUCTIVE + RENAMING, so the usual F-022 rule ("release the
-- backward-compatible binary first") cannot apply: a rename breaks both directions at
-- once. This is the coordinated break the plan calls Wave B. Take the seconds of downtime:
--
--     systemctl --user stop scoracle-cognition.service scoracle-api.service
--     DATABASE_PRIVATE_URL=… ./sql/migrate.sh
--     scripts/hosting/release.sh        # builds + installs + restarts all five binaries
--
-- Wave A (Rust, backend 929511d) is committed but was never deployed, so this release
-- ships Wave A + Wave B together — one build, one intended fleet-wide
-- rating→momentum→sigil regen (the prompt bumps are Wave A's; schedule like the mig-173
-- crown fold). The migration GATES on the daemon being stopped, not merely idle. After
-- applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

BEGIN;

-- 0. Lock discipline. Every statement here is metadata-only except the two JSONB
--    rewrites, but player_stats/stat_summaries are hot tables: fail loud and retry
--    rather than queue the fleet behind an ACCESS EXCLUSIVE wait.
SET LOCAL lock_timeout = '30s';

-- ---------------------------------------------------------------------------
-- 1. The gates (the 045/220 habit).
-- ---------------------------------------------------------------------------
-- The daemon must be STOPPED, not merely idle. Wave A (Rust, 929511d) and Wave B ship
-- in ONE release, so there is no "has Wave A drained yet" evidence to assert — the only
-- thing that keeps a pre-221 binary from writing to columns this migration drops is that
-- no such binary is running. Two independent proofs of quiet:
DO $$
DECLARE
    v_running int;
    v_fresh   int;
    v_last    timestamptz;
BEGIN
    -- 1a. Nothing mid-flight: the stage rename in step 9 rewrites the queue's primary
    --     key, and a running claim would come back to a row that moved.
    SELECT count(*) INTO v_running FROM public.pipeline_work WHERE status = 'running';
    IF v_running > 0 THEN
        RAISE EXCEPTION '221 refused: % pipeline_work row(s) are RUNNING — stop scoracle-cognition.service first', v_running;
    END IF;

    -- 1b. Nothing has touched the queue in the last minute. A live drain writes
    --     pipeline_work continuously, so a quiet minute is the observable signature of
    --     a stopped daemon (and 1a alone cannot tell "stopped" from "between claims").
    SELECT count(*), max(updated_at) INTO v_fresh, v_last
      FROM public.pipeline_work WHERE updated_at > now() - interval '1 minute';
    IF v_fresh > 0 THEN
        RAISE EXCEPTION '221 refused: % pipeline_work row(s) written in the last minute (latest %) — the daemon is still live; stop scoracle-cognition.service and scoracle-api.service first', v_fresh, v_last;
    END IF;
END $$;

-- ---------------------------------------------------------------------------
-- 2. Column drops. The specialist rail leaves storage.
-- ---------------------------------------------------------------------------
ALTER TABLE public.player_stats
    DROP COLUMN IF EXISTS rating_specialist,
    DROP COLUMN IF EXISTS rating_specialist_rank,
    DROP COLUMN IF EXISTS rating_specialist_score,
    DROP COLUMN IF EXISTS rating_specialty;

ALTER TABLE public.team_stats
    DROP COLUMN IF EXISTS rating_specialist,
    DROP COLUMN IF EXISTS rating_specialist_rank,
    DROP COLUMN IF EXISTS rating_specialist_score,
    DROP COLUMN IF EXISTS rating_specialty;

ALTER TABLE public.rating_history
    DROP COLUMN IF EXISTS rating_specialist,
    DROP COLUMN IF EXISTS rating_specialist_rank,
    DROP COLUMN IF EXISTS rating_specialist_score,
    DROP COLUMN IF EXISTS rating_specialty;

ALTER TABLE public.event_box_scores
    DROP COLUMN IF EXISTS rating_specialist,
    DROP COLUMN IF EXISTS rating_specialist_pct,
    DROP COLUMN IF EXISTS rating_specialty;

ALTER TABLE public.event_team_stats
    DROP COLUMN IF EXISTS rating_specialist,
    DROP COLUMN IF EXISTS rating_specialist_pct,
    DROP COLUMN IF EXISTS rating_specialty;

-- The Rating card's hero label. Code-owned since s18, unwritten since s19.
ALTER TABLE public.stat_summaries DROP COLUMN IF EXISTS divined_peak;

-- ---------------------------------------------------------------------------
-- 3. Column renames — §3d, uniform "rating". Idempotent: each rename is guarded by
--    the old name still existing, so a re-run is a no-op rather than an error.
-- ---------------------------------------------------------------------------
DO $$
DECLARE r record;
BEGIN
    FOR r IN
        SELECT * FROM (VALUES
            ('player_stats',     'rating_composite',           'rating'),
            ('player_stats',     'rating_composite_rank',      'rating_rank'),
            ('player_stats',     'rating_composite_score',     'rating_score'),
            ('team_stats',       'rating_composite',           'rating'),
            ('team_stats',       'rating_composite_rank',      'rating_rank'),
            ('team_stats',       'rating_composite_score',     'rating_score'),
            ('rating_history',   'rating_composite',           'rating'),
            ('rating_history',   'rating_composite_rank',      'rating_rank'),
            ('rating_history',   'rating_composite_score',     'rating_score'),
            ('event_box_scores', 'rating_composite',           'rating'),
            ('event_box_scores', 'rating_composite_pct',       'rating_pct'),
            ('event_team_stats', 'rating_composite',           'rating'),
            ('event_team_stats', 'rating_composite_pct',       'rating_pct'),
            ('stat_summaries',   'peak_trajectory',            'rating_trajectory'),
            ('stat_summaries',   'peak_trajectory_label',      'rating_trajectory_label'),
            ('stat_summaries',   'peak_trajectory_components', 'rating_trajectory_components')
        ) AS t(tbl, oldc, newc)
    LOOP
        IF EXISTS (
            SELECT 1 FROM information_schema.columns
            WHERE table_schema = 'public' AND table_name = r.tbl AND column_name = r.oldc
        ) THEN
            EXECUTE format('ALTER TABLE public.%I RENAME COLUMN %I TO %I', r.tbl, r.oldc, r.newc);
        END IF;
    END LOOP;
END $$;

-- 3b. Index + constraint NAMES. A column rename rewrites their definitions automatically
--     (they reference attnums) but leaves the names describing a retired concept.
DROP INDEX IF EXISTS public.idx_player_stats_rating_specialist;
DROP INDEX IF EXISTS public.idx_player_stats_rating_specialty;
DROP INDEX IF EXISTS public.idx_team_stats_rating_specialist;

DO $$
DECLARE r record;
BEGIN
    FOR r IN
        SELECT * FROM (VALUES
            ('idx_player_stats_rating_composite',  'idx_player_stats_rating'),
            ('idx_team_stats_rating_composite',    'idx_team_stats_rating'),
            ('idx_stat_summaries_peak_trajectory', 'idx_stat_summaries_rating_trajectory')
        ) AS t(oldn, newn)
    LOOP
        IF to_regclass('public.' || r.oldn) IS NOT NULL THEN
            EXECUTE format('ALTER INDEX public.%I RENAME TO %I', r.oldn, r.newn);
        END IF;
    END LOOP;

    FOR r IN
        SELECT * FROM (VALUES
            ('stat_summaries', 'stat_summaries_peak_trajectory_check',
                               'stat_summaries_rating_trajectory_check'),
            ('stat_summaries', 'stat_summaries_peak_trajectory_components_not_null',
                               'stat_summaries_rating_trajectory_components_not_null')
        ) AS t(tbl, oldn, newn)
    LOOP
        IF EXISTS (
            SELECT 1 FROM pg_constraint
            WHERE conrelid = format('public.%I', r.tbl)::regclass AND conname = r.oldn
        ) THEN
            EXECUTE format('ALTER TABLE public.%I RENAME CONSTRAINT %I TO %I', r.tbl, r.oldn, r.newn);
        END IF;
    END LOOP;
END $$;

-- ---------------------------------------------------------------------------
-- 4. The stored rating_modes payloads. Written through rating_mode_peak_payload(),
--    so every mode object carries peak/peak_rank/peak_score/peak_label plus
--    composite/composite_rank/composite_score. Drop the first four, rename the rest
--    into the uniform vocabulary. 26,726 rows on player_stats at time of writing —
--    a cheap rewrite, and the alternative (a read-side coalescer) is exactly the
--    back-compat debt this migration exists to pay off.
-- ---------------------------------------------------------------------------
UPDATE public.player_stats ps
   SET rating_modes = (
        SELECT jsonb_object_agg(
                   m.k,
                   (m.v - 'peak' - 'peak_rank' - 'peak_score' - 'peak_label'
                        - 'composite' - 'composite_rank' - 'composite_score')
                   || jsonb_strip_nulls(jsonb_build_object(
                          'rating',       m.v->'composite',
                          'rating_rank',  m.v->'composite_rank',
                          'rating_score', m.v->'composite_score'))
               )
        FROM jsonb_each(ps.rating_modes) AS m(k, v))
 WHERE ps.rating_modes IS NOT NULL
   AND ps.rating_modes::text ~ '"(peak|peak_rank|peak_score|peak_label|composite|composite_rank|composite_score)"';

-- ---------------------------------------------------------------------------
-- 5. The two back-compat helpers go. rating_mode_peak_payload() was the write-side
--    coalescer; rating_breakdown_without_specialty() stripped a flag the engine now
--    never emits (verified: 0 stored breakdowns contain is_specialty).
-- ---------------------------------------------------------------------------
DROP FUNCTION IF EXISTS public.rating_mode_peak_payload(jsonb);
DROP FUNCTION IF EXISTS public.rating_breakdown_without_specialty(jsonb);

-- ---------------------------------------------------------------------------
-- 6. The rating engine. _compute_rating_bundle's OUT columns change, so it must be
--    dropped and recreated (CREATE OR REPLACE cannot alter a return type). Its
--    internal CTE vocabulary keeps the word "composite" — that is local arithmetic
--    naming, not a product noun on any served surface.
-- ---------------------------------------------------------------------------
DROP FUNCTION IF EXISTS public._compute_rating_bundle(text, integer, text);

CREATE FUNCTION public._compute_rating_bundle(p_sport text, p_season integer, p_rate_mode text)
 RETURNS TABLE(player_id integer, league_id integer, composite numeric, composite_rank numeric, composite_score numeric, breakdown jsonb, scoped_ranks jsonb, scoped_scores jsonb)
 LANGUAGE sql
 STABLE
AS $function$
    WITH lasp AS (
        SELECT CASE WHEN p_sport='FOOTBALL'
                    THEN round(avg(NULLIF(stats->>'save_pct','')::numeric), 4) END AS asp
        FROM player_stats
        WHERE sport='FOOTBALL' AND season=p_season AND position='Goalkeeper'
          AND (stats->>'appearances')::numeric >= 15
    ),
    dp AS (
        SELECT ps.player_id, COALESCE(ps.league_id, 0) AS league_id, ps.position,
               tm.conference, tm.division,
               d.label, d.value, d.in_comp, d.in_spec, d.sign, d.facet,
               -- Phase 2: TAG eligibility instead of filtering it out. Sub-gate players
               -- still produce datapoints (for their breakdown); pop/ranks/scoped below
               -- use `WHERE is_ranked` so the rated cohort is unchanged.
               COALESCE((
                   SELECT bool_and(COALESCE((ps.stats->>rt.stat_key)::numeric, 0) >= rt.min_value)
                   FROM public.rating_thresholds rt WHERE rt.sport = p_sport
                 ), FALSE) AS is_ranked
        FROM player_stats ps
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        LEFT JOIN LATERAL (
            SELECT tts.stats->>'opp_possession_pct' AS opp
            FROM team_stats tts
            WHERE tts.team_id = ps.team_id AND tts.sport = p_sport AND tts.season = p_season
            LIMIT 1
        ) topp ON p_sport = 'FOOTBALL'
        CROSS JOIN lasp
        CROSS JOIN LATERAL rating_datapoints(
            p_sport,
            CASE WHEN p_sport = 'FOOTBALL'
                 THEN ps.stats || jsonb_strip_nulls(jsonb_build_object(
                          'team_opp_possession', topp.opp,
                          'league_avg_save_pct', lasp.asp))
                 ELSE ps.stats END,
            p_rate_mode, ps.position) d
        WHERE ps.sport = p_sport AND ps.season = p_season
    ),
    pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM dp WHERE is_ranked GROUP BY label
    ),
    z AS (
        SELECT d.player_id, d.league_id, d.position, d.conference, d.division,
               d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value, d.is_ranked,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT player_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY player_id, league_id
    ),
    comp_facet AS (
        SELECT player_id, league_id, SUM(facet_mean) AS composite
        FROM (
            SELECT player_id, league_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY player_id, league_id, facet
        ) fm
        GROUP BY player_id, league_id
    ),
    comp AS (
        SELECT player_id, league_id, composite FROM comp_flat
    ),
    rk AS (
        SELECT DISTINCT player_id, league_id, is_ranked FROM z
    ),
    scored AS (
        -- Rated cohort: percent_rank over the rated set only → byte-identical to before.
        SELECT player_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_position,
               CASE WHEN p_sport IN ('NFL','NBA') AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, conference ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_conference,
               CASE WHEN p_sport='NFL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, division ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_division,
               CASE WHEN p_sport='FOOTBALL' AND position IS NOT NULL
                    THEN ROUND((percent_rank() OVER (PARTITION BY label, position, league_id ORDER BY sign*zr ASC))::numeric*100,1) END AS pct_league
        FROM z WHERE is_ranked
        UNION ALL
        -- Sub-gate players: per-stat percentile = standing within the RATED cohort for
        -- that stat (count-based, so it doesn't perturb the cohort's own percent_rank).
        -- Scope cuts omitted (they are unranked).
        SELECT u.player_id, u.league_id, u.label, u.in_comp, u.in_spec, u.sign, u.facet, u.value, u.zr,
               -- Sub-gate fill = the datapoint's standardized magnitude vs the rated
               -- cohort (50 + 10*z in the good direction, clamped 1-99) — the same scale
               -- as rating_score. Fast scalar; a true percentile-vs-cohort is O(n^2).
               ROUND(LEAST(99.0, GREATEST(1.0, 50 + 10.0 * (u.sign * u.zr)))::numeric, 1) AS pct,
               NULL::numeric AS pct_position, NULL::numeric AS pct_conference,
               NULL::numeric AS pct_division, NULL::numeric AS pct_league
        FROM z u WHERE NOT u.is_ranked
    ),
    bd AS (
        -- (mig 221) the specialty flag leaves the datapoint with the concept. A client
        -- that ever wants a hero row can take max(z); the Scout names standouts in prose.
        -- (Spelling the retired key out here would trip this migration's own proof gate.)
        SELECT s.player_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet,
                   'scoped_pct', jsonb_strip_nulls(jsonb_build_object(
                       'position', s.pct_position, 'conference', s.pct_conference,
                       'division', s.pct_division, 'league', s.pct_league))
               ) ORDER BY s.label) AS breakdown
        FROM scored s
        GROUP BY s.player_id, s.league_id
    ),
    base AS (
        -- (mig 221) the specialist CTE was an INNER join here; without it an entity is
        -- rated on its composite alone, which is what the rating always was.
        SELECT c.player_id, c.league_id,
               ROUND(c.composite, 4) AS composite,
               bd.breakdown, rk.is_ranked
        FROM comp c
        JOIN bd USING (player_id, league_id)
        JOIN rk USING (player_id, league_id)
    ),
    ranks AS (
        SELECT player_id, league_id, is_ranked,
               CASE WHEN is_ranked THEN ROUND((percent_rank() OVER (PARTITION BY is_ranked ORDER BY composite ASC))::numeric * 100, 1) END AS composite_rank,
               CASE WHEN is_ranked THEN public.rating_score(composite, AVG(composite) OVER(PARTITION BY is_ranked), STDDEV_POP(composite) OVER(PARTITION BY is_ranked)) END AS composite_score
        FROM base
    ),
    scoped AS (
        SELECT b.player_id, b.league_id,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position ORDER BY b.composite ASC))::numeric*100,1) END AS pos_pct,
               CASE WHEN p_sport IN ('NFL','NBA') THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.conference ORDER BY b.composite ASC))::numeric*100,1) END AS conf_pct,
               CASE WHEN p_sport='NFL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, tm.division ORDER BY b.composite ASC))::numeric*100,1) END AS div_pct,
               CASE WHEN p_sport='FOOTBALL' THEN ROUND((percent_rank() OVER (PARTITION BY ps.position, ps.league_id ORDER BY b.composite ASC))::numeric*100,1) END AS league_pct,
               CASE WHEN p_sport IN ('NFL','FOOTBALL') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position)) END AS pos_score,
               CASE WHEN p_sport IN ('NFL','NBA') THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.conference), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.conference)) END AS conf_score,
               CASE WHEN p_sport='NFL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, tm.division), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, tm.division)) END AS div_score,
               CASE WHEN p_sport='FOOTBALL' THEN public.rating_score(b.composite, AVG(b.composite) OVER(PARTITION BY ps.position, ps.league_id), STDDEV_POP(b.composite) OVER(PARTITION BY ps.position, ps.league_id)) END AS league_score
        FROM base b
        JOIN player_stats ps
          ON ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
         AND COALESCE(ps.league_id, 0) = b.league_id
        LEFT JOIN teams tm ON tm.id = ps.team_id AND tm.sport = p_sport
        WHERE ps.position IS NOT NULL AND b.is_ranked
    )
    SELECT b.player_id, b.league_id,
           CASE WHEN b.is_ranked THEN b.composite END AS composite,
           r.composite_rank, r.composite_score,
           b.breakdown,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_pct, 'conference', sc.conf_pct,
               'division', sc.div_pct, 'league', sc.league_pct)), '{}'::jsonb) AS scoped_ranks,
           NULLIF(jsonb_strip_nulls(jsonb_build_object(
               'position', sc.pos_score, 'conference', sc.conf_score,
               'division', sc.div_score, 'league', sc.league_score)), '{}'::jsonb) AS scoped_scores
    FROM base b
    JOIN ranks r USING (player_id, league_id)
    LEFT JOIN scoped sc USING (player_id, league_id);
$function$;

CREATE OR REPLACE FUNCTION public.compute_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
    v_mode    TEXT;
    v_modes   TEXT[] := ARRAY['total'] || COALESCE(
        (SELECT array_agg(mode ORDER BY mode) FROM public.rate_modes WHERE sport = p_sport),
        ARRAY[]::TEXT[]);
BEGIN
    UPDATE player_stats
       SET rating = NULL, rating_rank = NULL, rating_score = NULL,
           rating_scoped_scores = NULL,
           rating_breakdown = NULL, rating_scoped_ranks = NULL, rating_modes = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating IS NOT NULL OR rating_rank IS NOT NULL OR rating_modes IS NOT NULL);

    FOREACH v_mode IN ARRAY v_modes LOOP
        IF v_mode = 'total' THEN
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, 'total')
            )
            UPDATE player_stats ps SET
                rating               = b.composite,
                rating_rank          = b.composite_rank,
                rating_score         = b.composite_score,
                rating_breakdown     = b.breakdown,
                rating_scoped_ranks  = b.scoped_ranks,
                rating_scoped_scores = b.scoped_scores
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
            GET DIAGNOSTICS v_updated = ROW_COUNT;
        ELSE
            WITH b AS MATERIALIZED (
                SELECT * FROM _compute_rating_bundle(p_sport, p_season, v_mode)
            )
            UPDATE player_stats ps SET
                rating_modes = COALESCE(ps.rating_modes, '{}'::jsonb) || jsonb_build_object(
                    v_mode,
                    jsonb_build_object(
                        'rating',        b.composite,
                        'rating_rank',   b.composite_rank,
                        'rating_score',  b.composite_score,
                        'breakdown',     b.breakdown,
                        'scoped_ranks',  b.scoped_ranks,
                        'scoped_scores', b.scoped_scores
                    ))
            FROM b
            WHERE ps.player_id = b.player_id AND ps.sport = p_sport AND ps.season = p_season
              AND COALESCE(ps.league_id, 0) = b.league_id;
        END IF;
    END LOOP;

    RETURN v_updated;
END;
$function$;

CREATE OR REPLACE FUNCTION public.compute_team_rating(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE team_stats
       SET rating = NULL, rating_rank = NULL, rating_score = NULL,
           rating_scoped_scores = NULL,
           rating_categories = NULL, rating_scoped_ranks = NULL, rating_breakdown = NULL
     WHERE sport = p_sport AND season = p_season
       AND (rating IS NOT NULL OR rating_rank IS NOT NULL);

    DROP TABLE IF EXISTS _team_dp;
    CREATE TEMP TABLE _team_dp (
        team_id INTEGER, league_id INTEGER, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    INSERT INTO _team_dp
    SELECT ts.team_id, COALESCE(ts.league_id, 0),
           dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM team_stats ts
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, ts.stats) dp
    WHERE ts.sport = p_sport AND ts.season = p_season AND ts.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.in_comp, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT team_id, league_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY team_id, league_id
    )
    UPDATE team_stats ts SET rating = ROUND(c.composite, 4)
    FROM composite c
    WHERE ts.team_id = c.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = c.league_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_dp GROUP BY label
    ),
    z AS (
        SELECT d.team_id, d.league_id, d.label, d.in_comp, d.in_spec, d.sign, d.facet, d.value,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_dp d JOIN pop p USING (label)
    ),
    scored AS (
        SELECT team_id, league_id, label, in_comp, in_spec, sign, facet, value, zr,
               ROUND((percent_rank() OVER (PARTITION BY label ORDER BY sign * zr ASC))::numeric * 100, 1) AS pct
        FROM z
    ),
    agg AS (
        SELECT s.team_id, s.league_id,
               jsonb_agg(jsonb_build_object(
                   'label', s.label, 'value', s.value, 'z', ROUND(s.zr, 4), 'pct', s.pct,
                   'in_comp', s.in_comp, 'in_spec', s.in_spec, 'sign', s.sign, 'facet', s.facet
               ) ORDER BY s.facet, s.label) AS breakdown
        FROM scored s
        GROUP BY s.team_id, s.league_id
    )
    UPDATE team_stats ts SET rating_breakdown = a.breakdown
    FROM agg a
    WHERE ts.team_id = a.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = a.league_id AND ts.rating IS NOT NULL;

    WITH r AS (
        SELECT team_id, league_id,
               ROUND((percent_rank() OVER (ORDER BY rating ASC))::numeric * 100, 1) AS crank,
               public.rating_score(rating, AVG(rating) OVER(), STDDEV_POP(rating) OVER()) AS cscore
        FROM team_stats
        WHERE sport = p_sport AND season = p_season AND rating IS NOT NULL
    )
    UPDATE team_stats ts SET rating_rank = r.crank, rating_score = r.cscore
    FROM r
    WHERE ts.team_id = r.team_id AND ts.sport = p_sport AND ts.season = p_season
      AND COALESCE(ts.league_id, 0) = r.league_id;

    RETURN v_updated;
END;
$function$;

CREATE OR REPLACE FUNCTION public.compute_event_starline(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated  INTEGER := 0;
    v_balanced BOOLEAN := FALSE;
BEGIN
    UPDATE event_box_scores
       SET rating = NULL
     WHERE sport = p_sport AND season = p_season
       AND rating IS NOT NULL;

    DROP TABLE IF EXISTS _starline_dp;
    CREATE TEMP TABLE _starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER, facet TEXT
    ) ON COMMIT DROP;

    -- Every participated event × the shared datapoint definitions (position-gated).
    INSERT INTO _starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign, dp.facet
    FROM event_box_scores e
    CROSS JOIN LATERAL rating_datapoints(p_sport, e.stats, 'total', e.position) dp
    WHERE e.sport = p_sport AND e.season = p_season
      AND (e.minutes_played IS NULL OR e.minutes_played > 0);

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.label, d.in_comp, d.sign, d.facet,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _starline_dp d JOIN pop p USING (label)
    ),
    comp_flat AS (
        SELECT event_id, SUM(sign * zr) AS composite
        FROM z WHERE in_comp GROUP BY event_id
    ),
    comp_facet AS (
        SELECT event_id, SUM(facet_mean) AS composite
        FROM (
            SELECT event_id, facet, AVG(sign * zr) AS facet_mean
            FROM z WHERE in_comp GROUP BY event_id, facet
        ) fm
        GROUP BY event_id
    ),
    composite AS (
        SELECT event_id, composite FROM comp_flat  WHERE NOT v_balanced
        UNION ALL
        SELECT event_id, composite FROM comp_facet WHERE     v_balanced
    )
    UPDATE event_box_scores e SET rating = ROUND(c.composite, 4)
    FROM composite c
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$function$;

CREATE OR REPLACE FUNCTION public.compute_team_event_starline(p_sport text, p_season integer)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_updated INTEGER := 0;
BEGIN
    UPDATE event_team_stats
       SET rating = NULL
     WHERE sport = p_sport AND season = p_season
       AND rating IS NOT NULL;

    DROP TABLE IF EXISTS _team_starline_dp;
    CREATE TEMP TABLE _team_starline_dp (
        event_id BIGINT, label TEXT,
        value NUMERIC, in_comp BOOLEAN, in_spec BOOLEAN, sign INTEGER
    ) ON COMMIT DROP;

    INSERT INTO _team_starline_dp
    SELECT e.id, dp.label, dp.value, dp.in_comp, dp.in_spec, dp.sign
    FROM event_team_stats e
    CROSS JOIN LATERAL rating_datapoints_team(p_sport, e.stats) dp
    WHERE e.sport = p_sport AND e.season = p_season AND e.stats <> '{}'::jsonb;

    WITH pop AS (
        SELECT label, AVG(value) AS mean, NULLIF(STDDEV_POP(value), 0) AS sd
        FROM _team_starline_dp GROUP BY label
    ),
    z AS (
        SELECT d.event_id, d.in_comp, d.sign, d.label,
               COALESCE((d.value - p.mean) / p.sd, 0) AS zr
        FROM _team_starline_dp d JOIN pop p USING (label)
    ),
    composite AS (
        SELECT event_id, SUM(sign * zr) AS composite FROM z WHERE in_comp GROUP BY event_id
    )
    UPDATE event_team_stats e SET rating = ROUND(c.composite, 4)
    FROM composite c
    WHERE e.id = c.event_id;
    GET DIAGNOSTICS v_updated = ROW_COUNT;

    RETURN v_updated;
END;
$function$;

CREATE OR REPLACE FUNCTION public.recalculate_event_rating_pct(p_sport text, p_season integer)
 RETURNS void
 LANGUAGE plpgsql
AS $function$
BEGIN
    -- Player events: clear stale pct, then percent_rank the live z's.
    UPDATE event_box_scores
       SET rating_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND rating_pct IS NOT NULL;

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating ASC))::numeric * 100, 1) AS cpct
        FROM event_box_scores
        WHERE sport = p_sport AND season = p_season AND rating IS NOT NULL
    )
    UPDATE event_box_scores e
       SET rating_pct = r.cpct
    FROM ranked r WHERE e.id = r.id;

    -- Team events (same, no position dimension to begin with — already flat).
    UPDATE event_team_stats
       SET rating_pct = NULL
     WHERE sport = p_sport AND season = p_season
       AND rating_pct IS NOT NULL;

    WITH ranked AS (
        SELECT id,
               ROUND((percent_rank() OVER (ORDER BY rating ASC))::numeric * 100, 1) AS cpct
        FROM event_team_stats
        WHERE sport = p_sport AND season = p_season AND rating IS NOT NULL
    )
    UPDATE event_team_stats e
       SET rating_pct = r.cpct
    FROM ranked r WHERE e.id = r.id;
END;
$function$;

CREATE OR REPLACE FUNCTION public.snapshot_rating_history(p_sport text, p_season integer, p_trigger text DEFAULT 'recompute'::text)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    v_inserted INTEGER := 0;
    v_count    INTEGER;
BEGIN
    -- Players
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating, rating_score, rating_rank,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'player', ps.player_id, ps.sport, ps.season, ps.league_id,
        ps.rating, ps.rating_score, ps.rating_rank,
        ps.season_composite_rank_alltime, ps.rating_modes, p_trigger
    FROM player_stats ps
    WHERE ps.sport = p_sport AND ps.season = p_season
      AND ps.rating IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'player' AND rh.entity_id = ps.player_id
            AND rh.sport = ps.sport AND rh.season = ps.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'player' AND rh2.entity_id = ps.player_id
                  AND rh2.sport = ps.sport AND rh2.season = ps.season)
            AND rh.rating       IS NOT DISTINCT FROM ps.rating
            AND rh.rating_score IS NOT DISTINCT FROM ps.rating_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    -- Teams (team_stats has no rating_modes column → NULL)
    INSERT INTO public.rating_history (
        entity_type, entity_id, sport, season, league_id,
        rating, rating_score, rating_rank,
        season_composite_rank_alltime, rating_modes, trigger_type)
    SELECT 'team', ts.team_id, ts.sport, ts.season, ts.league_id,
        ts.rating, ts.rating_score, ts.rating_rank,
        ts.season_composite_rank_alltime, NULL::jsonb, p_trigger
    FROM team_stats ts
    WHERE ts.sport = p_sport AND ts.season = p_season
      AND ts.rating IS NOT NULL
      AND NOT EXISTS (
          SELECT 1 FROM public.rating_history rh
          WHERE rh.entity_type = 'team' AND rh.entity_id = ts.team_id
            AND rh.sport = ts.sport AND rh.season = ts.season
            AND rh.generated_at = (
                SELECT max(rh2.generated_at) FROM public.rating_history rh2
                WHERE rh2.entity_type = 'team' AND rh2.entity_id = ts.team_id
                  AND rh2.sport = ts.sport AND rh2.season = ts.season)
            AND rh.rating       IS NOT DISTINCT FROM ts.rating
            AND rh.rating_score IS NOT DISTINCT FROM ts.rating_score
      );
    GET DIAGNOSTICS v_count = ROW_COUNT;
    v_inserted := v_inserted + v_count;

    RETURN v_inserted;
END;
$function$;

CREATE OR REPLACE FUNCTION public.mark_momentum_refresh_from_event_rating()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
BEGIN
    IF NEW.rating_pct IS NULL THEN
        RETURN NEW;
    END IF;

    IF TG_OP = 'INSERT' OR OLD.rating_pct IS DISTINCT FROM NEW.rating_pct THEN
        PERFORM public.mark_momentum_refresh_needed(NEW.sport, 'rating');
    END IF;
    RETURN NEW;
END;
$function$;

-- ---------------------------------------------------------------------------
-- 7. The memory cards. The prior-season read loses its divined label; what
--    survives the retirement is what the Scout actually earns — profile
--    distinctiveness plus the composite-only trajectory. Model-facing prompt
--    context only, never user-exposed, and the echo-chamber rule means banked
--    outputs in the old wording age out on their own. This also retires the
--    Rust-side descrub_memory_card shim (scout/mod.rs).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.stat_context_for_entity(p_sport text, p_entity_type text, p_entity_id integer, p_season integer)
 RETURNS text
 LANGUAGE sql
 STABLE
AS $function$
WITH prior_read AS (
    SELECT format('Our prior read: season %s scored this profile %s/100 for distinctiveness%s.',
               s.season, s.notability,
               CASE WHEN COALESCE(s.rating_trajectory_label, '') <> ''
                    THEN '; ' || s.rating_trajectory_label ELSE '' END) AS line
    FROM stat_summaries s
    WHERE s.entity_type = p_entity_type AND s.entity_id = p_entity_id
      AND s.sport = p_sport AND s.season < p_season
      AND s.body IS NOT NULL AND s.notability IS NOT NULL
    ORDER BY s.season DESC, s.generated_at DESC
    LIMIT 1
),
moves AS (
    SELECT format('Ground truth: %s on %s.',
               CASE WHEN p_entity_type = 'player'
                    THEN 'joined ' || tm.name
                    ELSE 'signed ' || pl.name END,
               to_char(g.applied_at, 'Mon DD YYYY')) AS line,
           g.applied_at
    FROM transfer_ground_truth g
    JOIN players pl ON pl.id = g.player_id AND pl.sport = g.sport
    JOIN teams tm ON tm.id = g.team_id AND tm.sport = g.sport
    WHERE g.sport = p_sport
      AND g.applied_at > now() - interval '180 days'
      AND ((p_entity_type = 'player' AND g.player_id = p_entity_id)
        OR (p_entity_type = 'team' AND g.team_id = p_entity_id))
    ORDER BY g.applied_at DESC
    LIMIT 3
),
matchups AS (
    SELECT format('Matchup memory: %s vs %s — %s/game vs a %s baseline (adjusted %s%s), n=%s games, reliability %s/100.',
               m.stat_key, tm.name,
               round(m.matchup_avg, 1), round(m.baseline_avg, 1),
               CASE WHEN m.shrunk_delta >= 0 THEN '+' ELSE '' END,
               round(m.shrunk_delta, 1),
               m.n_games, m.reliability) AS line,
           (m.reliability / 100.0) * abs(m.shrunk_delta) AS rank
    FROM stat_matchups m
    JOIN teams tm ON tm.id = m.object_id AND tm.sport = m.sport
    WHERE m.sport = p_sport AND m.scope = 'career'
      AND m.subject_type = p_entity_type AND m.subject_id = p_entity_id
      AND m.object_type = 'team'
      AND p_entity_type = 'player'
    ORDER BY rank DESC
    LIMIT 3
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT line FROM prior_read),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM matchups)), '');
$function$;

CREATE OR REPLACE FUNCTION public.narrative_context_for_entity(p_sport text, p_entity_type text, p_entity_id integer)
 RETURNS text
 LANGUAGE sql
 STABLE
AS $function$
WITH eps AS (
    SELECT e.*,
           CASE WHEN e.subject_type = p_entity_type AND e.subject_id = p_entity_id
                THEN e.object_type ELSE e.subject_type END AS other_type,
           CASE WHEN e.subject_type = p_entity_type AND e.subject_id = p_entity_id
                THEN e.object_id ELSE e.subject_id END AS other_id
    FROM narrative_episodes e
    WHERE e.sport = p_sport AND e.link_type = 'co_mention'
      AND ((e.subject_type = p_entity_type AND e.subject_id = p_entity_id)
        OR (e.object_type = p_entity_type AND e.object_id = p_entity_id))
),
named AS (
    SELECT eps.*, COALESCE(pl.name, tm.name, 'unknown') AS other_name
    FROM eps
    LEFT JOIN players pl ON eps.other_type = 'player' AND pl.id = eps.other_id
         AND pl.sport = p_sport
    LEFT JOIN teams tm ON eps.other_type = 'team' AND tm.id = eps.other_id
         AND tm.sport = p_sport
),
sealed AS (
    SELECT format('Prior story: %s — %s (%s, peak coverage %s/100).',
               other_name,
               CASE WHEN outcome = 'confirmed' THEN 'ended in a CONFIRMED move'
                    ELSE 'fizzled' END,
               CASE WHEN to_char(started_at, 'Mon YYYY') = to_char(ended_at, 'Mon YYYY')
                    THEN to_char(started_at, 'Mon YYYY')
                    ELSE to_char(started_at, 'Mon YYYY') || ' - ' || to_char(ended_at, 'Mon YYYY')
               END,
               peak_strength) AS line,
            ended_at
    FROM named
    WHERE status = 'sealed'
    ORDER BY ended_at DESC
    LIMIT 6
),
open_eps AS (
    SELECT format('Current story: %s — tracked since %s, peak coverage %s/100%s.',
               n.other_name, to_char(n.started_at, 'Mon DD'), n.peak_strength,
               COALESCE(', computed likelihood ' || n.likelihood || '/100', '')) AS line,
            COALESCE(n.likelihood, n.peak_strength) AS rank
    FROM named n
    WHERE n.status = 'open'
      AND NOT (p_entity_type = 'player' AND n.other_type = 'team' AND EXISTS (
          SELECT 1 FROM player_current_identity pci
          WHERE pci.sport = p_sport AND pci.player_id = p_entity_id
            AND pci.team_id = n.other_id))
    ORDER BY rank DESC
    LIMIT 5
),
moves AS (
    SELECT format('Ground truth: %s completed a confirmed move to %s on %s.',
               pl.name, tm.name, to_char(g.applied_at, 'Mon DD YYYY')) AS line,
            g.applied_at
    FROM transfer_ground_truth g
    JOIN players pl ON pl.id = g.player_id AND pl.sport = g.sport
    JOIN teams tm ON tm.id = g.team_id AND tm.sport = g.sport
    WHERE g.sport = p_sport
      AND g.applied_at > now() - interval '120 days'
      AND ((p_entity_type = 'player' AND g.player_id = p_entity_id)
        OR (p_entity_type = 'team' AND g.team_id = p_entity_id))
    ORDER BY g.applied_at DESC
    LIMIT 3
),
story_parts AS (
    -- (mig 219) The collapse of the thread lenses: one row per OPEN storyline
    -- this entity is an ACTIVE participant in (left_at IS NULL — D5: a part has
    -- its own lifespan, and an entity written out of a story stops remembering
    -- it), carrying the headline (latest packet's, falling back to the
    -- storyline's display title — packets are append-only snapshots, so the
    -- newest is the current state of the story), the membership report count,
    -- and the part's progression state (entries/sources/authority). One scan,
    -- two renderings below. Provenance-labeled continuity, NOT corroboration:
    -- it tells a voice which stories it is already inside, never that a claim
    -- is true. Membership counts are breadth, not measurement.
    SELECT se.storyline_id, se.role, se.joined_at, se.entry_count,
           se.distinct_sources, se.authority,
           COALESCE(se.last_progressed_at, s.last_seen_at) AS ord,
           COALESCE(NULLIF(p.headline, ''), NULLIF(s.title, ''), 'untitled') AS headline,
           m.n AS reports
    FROM storyline_entities se
    JOIN storylines s ON s.id = se.storyline_id
    LEFT JOIN LATERAL (
        SELECT pk.headline
        FROM packets pk
        WHERE pk.storyline_id = s.id
        ORDER BY pk.compiled_at DESC, pk.id DESC
        LIMIT 1
    ) p ON true
    CROSS JOIN LATERAL (
        SELECT count(*) AS n FROM storyline_articles sa WHERE sa.storyline_id = s.id
    ) m
    WHERE se.sport = p_sport AND se.entity_type = p_entity_type AND se.entity_id = p_entity_id
      AND se.left_at IS NULL
      AND s.status = 'open'
),
established AS (
    -- (mig 183 lineage, rebuilt mig 219) ESTABLISHED parts: source growth past
    -- the authority gate. They graduate OUT of the "Our story so far" block and
    -- render as one-line BACKGROUND FACTS — settled context the model may speak
    -- from, deliberately carrying NO impact/likelihood figures (source count +
    -- opening date are breadth and tenure, not measurement). Open storylines
    -- only: a resolved story's confirmation already renders as Ground truth.
    SELECT format('Established story (our archive, %s sources, since %s): "%s".',
               sp.distinct_sources,
               to_char(sp.joined_at, 'Mon DD'),
               sp.headline) AS line,
            sp.ord
    FROM story_parts sp
    WHERE sp.authority = 'established'
    ORDER BY sp.ord DESC
    LIMIT 2
),
own_story AS (
    -- (mig 182/211 lineage, rebuilt mig 219) CONTINUITY parts as progression:
    -- a header — headline, joined date, totals — plus the last 3 chapters,
    -- newest-first, each tagged with its OWN cited source count. A part the
    -- Journalist has not told yet renders the flat membership line (the mig 211
    -- shape) so a freshly-opened story is still remembered. Chapters join on
    -- (storyline_id, entity) — one entity's part in one story.
    SELECT CASE WHEN steps.txt IS NULL THEN
               format('Our story so far ("%s", opened %s, %s report%s%s).',
                   sp.headline,
                   to_char(sp.joined_at, 'Mon DD'),
                   sp.reports, CASE WHEN sp.reports = 1 THEN '' ELSE 's' END,
                   CASE WHEN COALESCE(sp.role, '') <> ''
                        THEN format(', this entity''s part: %s', sp.role) ELSE '' END)
           ELSE
               format('Our story so far ("%s", opened %s, %s entr%s, %s source%s%s):%s',
                   sp.headline,
                   to_char(sp.joined_at, 'Mon DD'),
                   sp.entry_count, CASE WHEN sp.entry_count = 1 THEN 'y' ELSE 'ies' END,
                   sp.distinct_sources, CASE WHEN sp.distinct_sources = 1 THEN '' ELSE 's' END,
                   CASE WHEN COALESCE(sp.role, '') <> ''
                        THEN format(', this entity''s part: %s', sp.role) ELSE '' END,
                   steps.txt)
           END AS line,
           sp.ord
    FROM story_parts sp
    LEFT JOIN LATERAL (
        SELECT E'\n' || string_agg(
                   format('  %s (%s source%s): %s, coverage %s/100',
                       to_char(c.generated_at, 'Mon DD'),
                       c.source_count, CASE WHEN c.source_count = 1 THEN '' ELSE 's' END,
                       replace(c.trajectory, '_', ' '),
                       c.impact),
                   E'\n' ORDER BY c.generated_at DESC, c.id DESC) AS txt
        FROM (
            SELECT s.id, s.generated_at, s.source_count, s.trajectory, s.impact
            FROM news_summaries s
            WHERE s.storyline_id = sp.storyline_id
              AND s.entity_type = p_entity_type AND s.entity_id = p_entity_id
              AND s.body IS NOT NULL AND s.impact IS NOT NULL
            ORDER BY s.generated_at DESC, s.id DESC
            LIMIT 3
        ) c
    ) steps ON true
    WHERE sp.authority = 'continuity'
    ORDER BY sp.ord DESC
    LIMIT 3
),
figures AS (
    -- Promoted (ACTIVE) news-derived people tied to this team — coaches, agents,
    -- executives the provider never seeds (mig 166). News-derived accumulation:
    -- graph-derived context, never ground truth.
    SELECT format('Team figure: %s (%s, news-derived, %s sources).',
               p.name, p.kind, p.distinct_sources) AS line,
            p.mention_count
    FROM narrative_persons p
    WHERE p.sport = p_sport AND p_entity_type = 'team' AND p.team_id = p_entity_id
      AND p.status = 'active' AND p.merged_into IS NULL
    ORDER BY p.mention_count DESC
    LIMIT 4
),
-- ------------------------------------------------------------------------------
-- OUR OWN SELF-HISTORY (outputs-as-memories, mig 168 + Phase 6): four lenses, all
-- provenance-labeled continuity, NEVER corroboration. Source-tagged where the lens banks it.
-- ------------------------------------------------------------------------------
own_transfer AS (
    -- The transfer lens's own recent staged reads (transfer_rumors). Players only.
    -- Freshest two = the recent read trajectory, source-tagged.
    SELECT format('Our prior read (transfer, %s%s): staged %s as %s%s.',
               to_char(r.generated_at, 'Mon DD'),
               CASE WHEN r.source_count > 0
                    THEN format(', %s source%s', r.source_count,
                                CASE WHEN r.source_count = 1 THEN '' ELSE 's' END)
                    ELSE '' END,
                t.name, r.stage,
                COALESCE(' (confidence ' || r.confidence || ')', '')) AS line,
            r.generated_at AS ord
    FROM transfer_rumors r
    JOIN teams t ON t.id = r.team_id AND t.sport = r.sport
    WHERE r.sport = p_sport AND p_entity_type = 'player' AND r.player_id = p_entity_id
      AND r.stage IS NOT NULL AND r.generated_at > now() - interval '30 days'
    ORDER BY r.generated_at DESC
    LIMIT 2
),
own_vibe AS (
    -- The vibe lens's own recent sentiment reads (vibe_scores). No source names banked;
    -- tag with the article count instead.
    SELECT format('Our prior read (mood, %s): mood %s/100%s.',
               to_char(v.generated_at, 'Mon DD'),
               v.sentiment,
               CASE WHEN array_length(v.input_news_ids, 1) > 0
                    THEN format(' (%s article%s)', array_length(v.input_news_ids, 1),
                                CASE WHEN array_length(v.input_news_ids, 1) = 1 THEN '' ELSE 's' END)
                    ELSE '' END) AS line,
            v.generated_at AS ord
    FROM vibe_scores v
    WHERE v.sport = p_sport AND v.entity_type = p_entity_type AND v.entity_id = p_entity_id
      AND v.sentiment IS NOT NULL AND v.generated_at > now() - interval '45 days'
    ORDER BY v.generated_at DESC
    LIMIT 2
),
own_momentum AS (
    -- The momentum lens's own recent reads (momentum_summaries).
    SELECT format('Our prior read (momentum, %s): %s%s.',
               to_char(m.generated_at, 'Mon DD'),
               m.direction,
               COALESCE(' (score ' || m.score || ')', '')) AS line,
            m.generated_at AS ord
    FROM momentum_summaries m
    WHERE m.sport = p_sport AND m.entity_type = p_entity_type AND m.entity_id = p_entity_id
      AND m.direction IS NOT NULL AND m.generated_at > now() - interval '45 days'
    ORDER BY m.generated_at DESC
    LIMIT 2
),
own_rating AS (
    -- (mig 221) The rating lens's latest banked read (stat_summaries). Least-weighted
    -- (stats-heavy) — the tail line. Season-keyed, so just the latest. The divined
    -- top-skill label retired with PEAK; distinctiveness and the trajectory remain.
    SELECT format('Our prior read (rating, season %s): profile distinctiveness %s/100%s.',
               s.season, s.notability,
               CASE WHEN COALESCE(s.rating_trajectory_label, '') <> ''
                    THEN '; ' || s.rating_trajectory_label ELSE '' END) AS line
    FROM stat_summaries s
    WHERE s.sport = p_sport AND s.entity_type = p_entity_type AND s.entity_id = p_entity_id
      AND s.body IS NOT NULL AND s.notability IS NOT NULL
    ORDER BY s.season DESC, s.generated_at DESC
    LIMIT 1
)
SELECT NULLIF(concat_ws(E'\n',
    (SELECT string_agg(line, E'\n' ORDER BY ended_at DESC) FROM sealed),
    (SELECT string_agg(line, E'\n' ORDER BY rank DESC) FROM open_eps),
    (SELECT string_agg(line, E'\n' ORDER BY applied_at DESC) FROM moves),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM established),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_story),
    (SELECT string_agg(line, E'\n' ORDER BY mention_count DESC) FROM figures),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_transfer),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_vibe),
    (SELECT string_agg(line, E'\n' ORDER BY ord DESC) FROM own_momentum),
    (SELECT line FROM own_rating)), '');
$function$;

-- ---------------------------------------------------------------------------
-- 8. refresh_momentum_scores — the rating rail's percentile column renamed under it.
--    (Body otherwise byte-identical to the prod definition.)
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport text DEFAULT NULL::text)
 RETURNS integer
 LANGUAGE plpgsql
AS $function$
DECLARE
    inserted_count INTEGER;
BEGIN
    -- Single-flight: the NOTIFY listener and the catch-up ticker can race a
    -- drain for the same sport. The loser returns NULL (NOT 0) — the Go drain
    -- leaves the dirty marker in place on NULL so the refresh is retried,
    -- never double-appended and never silently lost.
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN
        RETURN NULL;
    END IF;

    WITH target_sports AS (
        SELECT id AS sport, current_season
        FROM public.sports
        WHERE p_sport IS NULL OR id = upper(p_sport)
    ),
    -- Vibe window: a plain 21 calendar days. News sentiment flows through the
    -- offseason, so this clock never pauses with the fixture calendar.
    vibe AS (
        SELECT entity_type, entity_id, sport,
               ((array_agg(sentiment ORDER BY generated_at DESC))[1]
                - (array_agg(sentiment ORDER BY generated_at ASC))[1])::numeric AS vibe_slope,
               count(*)::int AS vibe_samples,
               min(generated_at) AS vibe_window_start,
               max(generated_at) AS vibe_window_end
        FROM public.vibe_scores
        WHERE sentiment IS NOT NULL
          AND generated_at > NOW() - INTERVAL '21 days'
          AND sport IN (SELECT sport FROM target_sports)
        GROUP BY entity_type, entity_id, sport
        HAVING count(*) >= 3
    ),
    -- Rating lookback: the entity's last season_bridge_window(sport) rated
    -- games (~10% of the season — the shared mig-025 schedule), across
    -- (current, previous) seasons so the lookback closes at a season's end
    -- and resumes at the next season's first game. Game-count, not calendar:
    -- bye weeks and schedule gaps cannot starve it.
    player_ranked AS (
        SELECT e.player_id AS entity_id, e.sport, e.season,
               e.rating_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.player_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_box_scores e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    team_ranked AS (
        SELECT e.team_id AS entity_id, e.sport, e.season,
               e.rating_pct, f.start_time,
               row_number() OVER (
                   PARTITION BY e.team_id, e.sport
                   ORDER BY f.start_time DESC
               ) AS rn
        FROM public.event_team_stats e
        JOIN public.fixtures f ON f.id = e.fixture_id
        JOIN target_sports ts ON ts.sport = e.sport
        WHERE e.rating_pct IS NOT NULL
          AND e.season IN (ts.current_season, ts.current_season - 1)
    ),
    player_rating AS (
        SELECT 'player'::text AS entity_type, pr.entity_id, pr.sport,
               max(pr.season) AS season,
               ((array_agg(pr.rating_pct ORDER BY pr.start_time DESC))[1]
                - (array_agg(pr.rating_pct ORDER BY pr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(pr.start_time) AS rating_window_start,
               max(pr.start_time) AS rating_window_end
        FROM player_ranked pr
        WHERE pr.rn <= public.season_bridge_window(pr.sport)
        GROUP BY pr.entity_id, pr.sport
        HAVING count(*) >= 2
    ),
    team_rating AS (
        SELECT 'team'::text AS entity_type, tr.entity_id, tr.sport,
               max(tr.season) AS season,
               ((array_agg(tr.rating_pct ORDER BY tr.start_time DESC))[1]
                - (array_agg(tr.rating_pct ORDER BY tr.start_time ASC))[1])::numeric AS rating_slope,
               count(*)::int AS rating_samples,
               min(tr.start_time) AS rating_window_start,
               max(tr.start_time) AS rating_window_end
        FROM team_ranked tr
        WHERE tr.rn <= public.season_bridge_window(tr.sport)
        GROUP BY tr.entity_id, tr.sport
        HAVING count(*) >= 2
    ),
    rating AS (
        SELECT * FROM player_rating
        UNION ALL
        SELECT * FROM team_rating
    ),
    entity_scores AS (
        SELECT COALESCE(v.entity_type, r.entity_type) AS entity_type,
               COALESCE(v.entity_id, r.entity_id) AS entity_id,
               COALESCE(v.sport, r.sport) AS sport,
               r.season,
               v.vibe_slope, COALESCE(v.vibe_samples, 0) AS vibe_samples,
               v.vibe_window_start, v.vibe_window_end,
               r.rating_slope, COALESCE(r.rating_samples, 0) AS rating_samples,
               r.rating_window_start, r.rating_window_end
        FROM vibe v
        FULL OUTER JOIN rating r
          ON r.entity_type = v.entity_type
         AND r.entity_id = v.entity_id
         AND r.sport = v.sport
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           -- SIGNED: the average of the present slopes, sign preserved. Falls
           -- are as much a historic datapoint as rises — this number is the
           -- durable per-snapshot momentum record, so it must not clamp
           -- downside.
           round((
               COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0)
           ) / NULLIF(
               (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
               + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
               0
           ), 3) AS momentum_score
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$function$;

-- ---------------------------------------------------------------------------
-- 9. The queue stage. `peak` was the stage's literal name; it becomes `rating`.
--    Safe as a plain UPDATE: the PK is (stage, entity_type, entity_id, sport) and
--    no `rating` rows exist yet (asserted below), and gate 1a already proved
--    nothing is mid-claim. The Rust/Go input_version prefixes follow in the same
--    release; every affected row regenerates anyway on the Wave A prompt bump.
-- ---------------------------------------------------------------------------
DO $$
DECLARE v_clash int;
BEGIN
    SELECT count(*) INTO v_clash FROM public.pipeline_work WHERE stage = 'rating';
    IF v_clash > 0 THEN
        RAISE EXCEPTION '221 refused: % pipeline_work row(s) already use stage ''rating'' — resolve the collision before renaming', v_clash;
    END IF;
END $$;

UPDATE public.pipeline_work
   SET stage = 'rating',
       input_version = 'rating:' || substring(input_version from 6)
 WHERE stage = 'peak';

-- ---------------------------------------------------------------------------
-- 10. Proof. Nothing PEAK-shaped may survive in the rating surface.
-- ---------------------------------------------------------------------------
DO $$
DECLARE
    v_cols  int;
    v_funcs int;
    v_modes int;
BEGIN
    SELECT count(*) INTO v_cols FROM information_schema.columns
     WHERE table_schema = 'public'
       AND column_name ~ '(rating_specialist|rating_specialty|divined_peak|peak_trajectory|rating_composite)';
    IF v_cols > 0 THEN
        RAISE EXCEPTION '221 failed: % PEAK-shaped column(s) survive', v_cols;
    END IF;

    SELECT count(*) INTO v_funcs FROM pg_proc p
      JOIN pg_namespace n ON n.oid = p.pronamespace
     WHERE n.nspname = 'public' AND p.prokind = 'f' AND p.proname <> 'array_agg'
       AND p.prosrc ~ '(rating_specialist|rating_specialty|divined_peak|peak_trajectory|rating_composite|is_specialty)';
    IF v_funcs > 0 THEN
        RAISE EXCEPTION '221 failed: % function(s) still reference a retired rating column', v_funcs;
    END IF;

    SELECT count(*) INTO v_modes FROM public.player_stats
     WHERE rating_modes IS NOT NULL
       AND rating_modes::text ~ '"(peak|peak_rank|peak_score|peak_label|composite|composite_rank|composite_score)"';
    IF v_modes > 0 THEN
        RAISE EXCEPTION '221 failed: % rating_modes payload(s) still carry retired keys', v_modes;
    END IF;
END $$;

-- 11. Self-record INSIDE this transaction (apply + record atomic).
INSERT INTO public.schema_migrations(version) VALUES ('221_peak_retirement')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: scripts/hosting/snapshot-schema.sh, then commit sql/schema/ with this file.

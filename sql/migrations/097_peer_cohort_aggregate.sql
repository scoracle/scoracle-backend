-- 097_peer_cohort_aggregate.sql  (Optimization Ledger O1)
--
-- /momentum (trendsStatement) recomputes the peer-cohort season aggregate LIVE on
-- every read: for the requesting entity's cohort — (sport, season, league, position)
-- for players; (sport, season, league) for teams — it scans every cohort member,
-- jsonb_each-explodes their stats, normalizes cumulative_total keys per-member by
-- games/matches_played, and AVGs per key. Measured ~17.6 ms for an NBA guard cohort
-- (248 members → 9,150 jsonb pairs) — and it is IDENTICAL for every entity in the
-- cohort save the leave-one-out of self.
--
-- This table precomputes the cohort once per refresh. To preserve EXACT semantics
-- (the read currently averages the cohort EXCLUDING the requesting entity), we store
-- per-key SUMS and COUNTS over the FULL cohort, plus the season-composite score sum/
-- count and the member count. The read then reconstructs the exact leave-one-out
-- average by subtracting the entity's own already-computed normalized value
-- (entity_season_aggregate, same normalization formula) and decrementing the count.
-- Result: same numbers as today, but the 248-member scan becomes one indexed row
-- lookup + a walk over the entity's own ~40-70 keys.
--
-- Cohort key notes:
--   * position '' is the sentinel for teams (team cohorts are league-wide, no position).
--     Players always carry a non-null position (NULL-position players match no cohort
--     today and still match none here).
--   * league_id is stored raw. NBA/NFL are uniformly league_id = 0; football splits by
--     league {8,82,301,384,564}. The read looks up COALESCE(effective_league, 0), which
--     mirrors trendsStatement's effective_league resolution (entity's own league for
--     football, 0 for NBA/NFL).
--   * Divisor: only teams have cumulative_total comparable keys; players have none, so
--     player keys are stored raw. Members with the divisor = 0 contribute NULL for a
--     cumulative key and are excluded from that key's sum AND count (mirrors AVG skipping
--     NULL), so leave-one-out counts stay exact.

CREATE TABLE IF NOT EXISTS public.peer_cohort_aggregate (
    sport        text    NOT NULL,
    season       integer NOT NULL,
    league_id    integer NOT NULL,
    entity_type  text    NOT NULL CHECK (entity_type IN ('player','team')),
    position     text    NOT NULL DEFAULT '',
    key_sums     jsonb   NOT NULL DEFAULT '{}'::jsonb,  -- {stat_key: SUM(normalized value)} over full cohort
    key_cnts     jsonb   NOT NULL DEFAULT '{}'::jsonb,  -- {stat_key: COUNT(members with that key)}
    score_sum    numeric,                               -- SUM(season_composite_score) over full cohort
    score_cnt    integer NOT NULL DEFAULT 0,            -- COUNT(members with non-null season_composite_score)
    member_count integer NOT NULL DEFAULT 0,            -- COUNT(*) cohort members (full, incl. self)
    computed_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sport, season, league_id, entity_type, position)
);

COMMENT ON TABLE public.peer_cohort_aggregate IS
    'O1: precomputed per-cohort season aggregates for /momentum. Stores full-cohort SUMS+COUNTS so the read reconstructs exact leave-one-out peer averages. Refreshed by refresh_peer_cohort_aggregates().';

-- refresh_peer_cohort_aggregates() — full rebuild. DELETE+INSERT in one transaction
-- (MVCC-safe: readers see the prior snapshot until commit; no ACCESS EXCLUSIVE lock,
-- unlike TRUNCATE), so it is safe to run while the read path depends on the table.
CREATE OR REPLACE FUNCTION public.refresh_peer_cohort_aggregates()
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    n integer;
BEGIN
    DELETE FROM public.peer_cohort_aggregate;

    INSERT INTO public.peer_cohort_aggregate
        (sport, season, league_id, entity_type, position,
         key_sums, key_cnts, score_sum, score_cnt, member_count, computed_at)
    WITH members AS (
        SELECT ps.sport, ps.season, ps.league_id, 'player'::text AS entity_type,
               ps.position, ps.stats, ps.season_composite_score
        FROM public.player_stats ps
        WHERE ps.position IS NOT NULL
        UNION ALL
        SELECT ts.sport, ts.season, ts.league_id, 'team'::text,
               ''::text AS position, ts.stats, ts.season_composite_score
        FROM public.team_stats ts
    ),
    exploded AS (
        SELECT m.sport, m.season, m.league_id, m.entity_type, m.position, kv.key,
               CASE
                   WHEN sd.unit = 'cumulative_total' THEN
                       (kv.value)::numeric
                       / NULLIF((m.stats->>(CASE WHEN m.sport = 'FOOTBALL'
                                                 THEN 'matches_played' ELSE 'games_played' END))::numeric, 0)
                   ELSE (kv.value)::numeric
               END AS val
        FROM members m
        CROSS JOIN LATERAL jsonb_each(m.stats) kv
        JOIN public.stat_definitions sd
          ON sd.sport = m.sport
         AND sd.entity_type = m.entity_type
         AND sd.key_name = kv.key
         AND sd.comparable = true
        WHERE jsonb_typeof(kv.value) = 'number'
    ),
    key_agg AS (
        SELECT sport, season, league_id, entity_type, position, key,
               SUM(val) AS sum_val, COUNT(*) AS cnt_val
        FROM exploded
        WHERE val IS NOT NULL          -- divisor=0 → NULL → excluded from sum AND count
        GROUP BY 1,2,3,4,5,6
    ),
    keyed AS (
        SELECT sport, season, league_id, entity_type, position,
               jsonb_object_agg(key, sum_val) AS key_sums,
               jsonb_object_agg(key, cnt_val) AS key_cnts
        FROM key_agg
        GROUP BY 1,2,3,4,5
    ),
    sized AS (
        SELECT sport, season, league_id, entity_type, position,
               COUNT(*) AS member_count,
               SUM(season_composite_score) AS score_sum,
               COUNT(season_composite_score) AS score_cnt
        FROM members
        GROUP BY 1,2,3,4,5
    )
    SELECT s.sport, s.season, s.league_id, s.entity_type, s.position,
           COALESCE(k.key_sums, '{}'::jsonb),
           COALESCE(k.key_cnts, '{}'::jsonb),
           s.score_sum, s.score_cnt, s.member_count, now()
    FROM sized s
    LEFT JOIN keyed k USING (sport, season, league_id, entity_type, position);

    GET DIAGNOSTICS n = ROW_COUNT;
    RETURN n;
END
$$;

-- Populate on apply (prod + any fresh env that replays this migration).
SELECT public.refresh_peer_cohort_aggregates();

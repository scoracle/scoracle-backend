-- 245: football season aggregation goes key-agnostic — the z-model's format shift.
--
-- Scott, 2026-09-07: "likely change up our z-score model to work with the new
-- format." The percentile sweep was always key-agnostic; the season AGGREGATORS
-- were not — football.aggregate_player_season hand-enumerated the vendor-era
-- vocabulary and gated on the minutes_played COLUMN, so FPL-era event rows
-- (full payload in stats jsonb, minutes as a key) aggregated to empty shells
-- ({"fantasy_points": 0.00} was the whole 2026 season row).
--
-- Both aggregators now sum whatever numeric keys the events carry:
--   * minutes come from the column OR the stats key (vendor and FPL eras both);
--   * percent-shaped keys (_pct/percentage/accuracy/rate) AVERAGE, never sum;
--   * per_90 / per_game keys derive ONLY where a stat_definitions row for the
--     derived key exists — the definitions stay the curation surface (mig 244);
--   * appearances/minutes_played are explicit overrides so both eras agree.
-- New stats flow by adding a definition, never by editing this function again.
BEGIN;

CREATE OR REPLACE FUNCTION football.aggregate_player_season(p_player_id integer, p_season integer, p_league_id integer DEFAULT 0)
RETURNS jsonb
LANGUAGE sql
STABLE
AS $function$
WITH raw AS (
    SELECT stats,
           COALESCE(minutes_played, (stats->>'minutes_played')::numeric, 0) AS mins
    FROM public.event_box_scores
    WHERE player_id = p_player_id
      AND sport = 'FOOTBALL'
      AND season = p_season
      AND league_id = p_league_id
),
played AS (SELECT * FROM raw WHERE mins > 0),
meta AS (
    SELECT COUNT(*)::numeric AS matches, COALESCE(SUM(mins), 0) AS minutes FROM played
),
sums AS (
    SELECT kv.key,
           CASE WHEN kv.key ~ '(_pct|percentage|accuracy|rate)$'
                THEN AVG(kv.value::numeric)
                ELSE SUM(kv.value::numeric)
           END AS total
    FROM played, jsonb_each_text(stats) AS kv(key, value)
    WHERE kv.value ~ '^-?[0-9]+(\.[0-9]+)?$'
    GROUP BY kv.key
),
base AS (
    SELECT COALESCE(jsonb_object_agg(key, ROUND(total, 2)), '{}'::jsonb) AS obj FROM sums
),
derived AS (
    SELECT COALESCE(jsonb_object_agg(d.key_name, ROUND(
               CASE WHEN d.key_name LIKE '%\_per\_90'
                    THEN s.total * 90.0 / NULLIF(m.minutes, 0)
                    ELSE s.total / NULLIF(m.matches, 0)
               END, 2)), '{}'::jsonb) AS obj
    FROM sums s
    CROSS JOIN meta m
    JOIN public.stat_definitions d
      ON d.sport = 'FOOTBALL' AND d.entity_type = 'player' AND d.is_derived
     AND (d.key_name = s.key || '_per_90' OR d.key_name = s.key || '_per_game')
    WHERE m.minutes > 0
)
SELECT CASE
    WHEN m.matches = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        b.obj
        || COALESCE(d.obj, '{}'::jsonb)
        || jsonb_build_object(
               'appearances', m.matches::int,
               'minutes_played', ROUND(m.minutes, 1))
        || CASE WHEN b.obj ? 'lineups' THEN '{}'::jsonb
                ELSE jsonb_build_object('lineups', m.matches::int) END)
END
FROM meta m, base b LEFT JOIN derived d ON true
$function$;

CREATE OR REPLACE FUNCTION football.aggregate_team_season(p_team_id integer, p_season integer, p_league_id integer DEFAULT 0)
RETURNS jsonb
LANGUAGE sql
STABLE
AS $function$
WITH raw AS (
    SELECT stats
    FROM public.event_team_stats
    WHERE team_id = p_team_id
      AND sport = 'FOOTBALL'
      AND season = p_season
      AND league_id = p_league_id
),
meta AS (SELECT COUNT(*)::numeric AS matches FROM raw),
sums AS (
    SELECT kv.key,
           CASE WHEN kv.key ~ '(_pct|percentage|accuracy|rate)$'
                THEN AVG(kv.value::numeric)
                ELSE SUM(kv.value::numeric)
           END AS total
    FROM raw, jsonb_each_text(stats) AS kv(key, value)
    WHERE kv.value ~ '^-?[0-9]+(\.[0-9]+)?$'
    GROUP BY kv.key
),
base AS (
    SELECT COALESCE(jsonb_object_agg(key, ROUND(total, 2)), '{}'::jsonb) AS obj FROM sums
)
SELECT CASE
    WHEN m.matches = 0 THEN '{}'::jsonb
    ELSE jsonb_strip_nulls(
        b.obj || jsonb_build_object('matches_played', m.matches::int))
END
FROM meta m, base b
$function$;

COMMIT;

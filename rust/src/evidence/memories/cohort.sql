-- Season-grain cohort context derived by the DuckDB engine and stored by the
-- analytics-snapshot batch job (migration 255). Season-grain and recomputable
-- from player_stats/team_stats ratings, so it refreshes on the same cadence
-- and semantics as the performance snapshot block; the observation time is
-- the snapshot's own computed_at.
SELECT a.season, a.league_id,
       a.computed_at::text AS observed_at,
       floor(extract(epoch FROM a.computed_at))::bigint AS observed_unix,
       jsonb_strip_nulls(jsonb_build_object(
           'season', a.season,
           'competition', l.name,
           'rating', round(a.rating::numeric, 2),
           'prior_season', a.prior_season,
           'prior_rating', round(a.prior_rating::numeric, 2),
           'delta', round(a.delta::numeric, 2),
           'delta_percentile', round(a.delta_pctile::numeric, 1),
           'peer_count', a.peer_count,
           'peer_delta', jsonb_strip_nulls(jsonb_build_object(
               'median', round(a.peer_delta_median::numeric, 2),
               'p25', round(a.peer_delta_p25::numeric, 2),
                'p75', round(a.peer_delta_p75::numeric, 2)))
        )) AS data
FROM public.analytics_entity_context a
LEFT JOIN public.leagues l ON l.id = a.league_id AND l.sport = a.sport
WHERE a.sport = $1 AND a.entity_type = $2 AND a.entity_id = $3 AND a.season <= $4
ORDER BY a.season DESC
LIMIT 5

COPY (
WITH latest AS (
    SELECT DISTINCT ON (sport, player_id, team_id)
        *
    FROM public.transfer_rumors
    WHERE is_rumor IS TRUE
      AND direction = 'incoming'
    ORDER BY sport, player_id, team_id, generated_at DESC
),
joined AS (
    SELECT
        l.*,
        p.name AS player_name_for_match,
        lower(p.name) AS full_player_key,
        lower(
            (regexp_split_to_array(p.name, '\s+'))[
                array_length(regexp_split_to_array(p.name, '\s+'), 1)
            ]
        ) AS last_player_key,
        newt.name AS new_team,
        lower(newt.name) AS full_key,
        lower(
            (regexp_split_to_array(newt.name, '\s+'))[
                array_length(regexp_split_to_array(newt.name, '\s+'), 1)
            ]
        ) AS last_key
    FROM latest l
    JOIN public.players p
      ON p.sport = l.sport
     AND p.id = l.player_id
    JOIN public.teams newt
      ON newt.sport = l.sport
     AND newt.id = l.team_id
),
evidence AS (
    SELECT
        j.id,
        count(DISTINCT lower(na.source)) FILTER (
            WHERE EXISTS (
                SELECT 1
                FROM unnest(ARRAY[
                    j.full_key,
                    CASE
                        WHEN length(j.last_key) > 3
                         AND j.last_key NOT IN ('city', 'fc', 'united', 'club', 'town', 'county')
                            THEN j.last_key
                        ELSE NULL
                    END
                ]) AS k(key)
                WHERE key IS NOT NULL
                  AND EXISTS (
                    SELECT 1
                    FROM unnest(ARRAY[
                        CASE
                            WHEN position(' ' in j.full_player_key) > 0
                                THEN j.full_player_key
                            ELSE NULL
                        END,
                        CASE
                            WHEN position(' ' in j.full_player_key) > 0
                             AND length(j.last_player_key) > 3
                                THEN j.last_player_key
                            ELSE NULL
                        END
                    ]) AS pk(player_key)
                    WHERE player_key IS NOT NULL
                      AND lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                          ~ (
                            '(^|[^[:alnum:]])'
                            || regexp_replace(player_key, '([.^$*+?()[{\|])', '\\\1', 'g')
                            || '([^[:alnum:]]|$)'
                          )
                  )
                  AND CASE
                    WHEN j.sport IN ('NBA', 'NFL') THEN
                        lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%finalizing trade with ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%finalized trade with ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%trade with ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%trade to ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%traded to ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%' || key || ' acquired%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%' || key || ' acquire%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%' || key || ' traded for%')
                    ELSE
                        lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%' || key || ' agree deal%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%' || key || ' agreed deal%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%agreement with ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%signing for ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%signs for ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%joins ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%join ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%transfer to ' || key || '%')
                        OR lower(coalesce(na.title, '') || ' ' || coalesce(na.description, ''))
                            LIKE ('%move to ' || key || '%')
                  END
            )
        ) AS confirmed_sources
    FROM joined j
    LEFT JOIN public.news_articles na
      ON na.id = ANY(j.input_news_ids)
    GROUP BY j.id
),
scored AS (
    SELECT
        j.sport,
        j.player_id,
        p.name AS player_name,
        pci.team_id AS old_team_id,
        oldt.name AS old_team,
        j.team_id AS new_team_id,
        j.new_team,
        j.id AS source_rumor_id,
        CASE
            WHEN coalesce(e.confirmed_sources, 0) >= 1
                THEN 'raw_heat_threshold_with_confirmed_hint'
            ELSE 'normal_heat'
        END AS trigger_reason,
        coalesce(j.heat, 0) AS raw_heat,
        coalesce(j.heat, 0) AS identity_heat,
        coalesce(j.heat, 0)::numeric / 100 AS deterministic_confidence,
        coalesce(e.confirmed_sources, 0) AS confirmed_sources,
        j.stage,
        j.confidence AS model_confidence,
        j.source_count,
        j.generated_at,
        j.rumor_updated_at,
        j.model_summary,
        th.min_heat,
        th.min_deterministic_confidence,
        row_number() OVER (
            PARTITION BY j.sport, j.player_id
            ORDER BY
                coalesce(j.heat, 0) DESC,
                j.generated_at DESC,
                j.id DESC
        ) AS player_candidate_rank
    FROM joined j
    JOIN public.players p
      ON p.sport = j.sport
     AND p.id = j.player_id
    JOIN public.player_current_identity pci
      ON pci.sport = j.sport
     AND pci.player_id = j.player_id
    JOIN public.teams oldt
      ON oldt.sport = pci.sport
     AND oldt.id = pci.team_id
    JOIN public.transfer_identity_thresholds th
      ON th.sport = j.sport
    LEFT JOIN evidence e
      ON e.id = j.id
    WHERE pci.team_id IS DISTINCT FROM j.team_id
)
SELECT
    sport,
    player_id,
    player_name,
    old_team_id,
    old_team,
    new_team_id,
    new_team,
    source_rumor_id,
    trigger_reason,
    raw_heat,
    identity_heat,
    deterministic_confidence,
    confirmed_sources,
    stage,
    model_confidence,
    source_count,
    generated_at,
    rumor_updated_at,
    player_candidate_rank,
    model_summary
FROM scored
WHERE identity_heat >= min_heat
  AND deterministic_confidence >= min_deterministic_confidence
ORDER BY sport, player_name, player_candidate_rank, identity_heat DESC, generated_at DESC
) TO STDOUT WITH (FORMAT csv, HEADER true);

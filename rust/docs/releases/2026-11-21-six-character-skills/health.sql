-- Production rows since the November 21 cutover. Only finished, nonempty readings count.
WITH cards AS (
    SELECT 'Journalist' AS character, sport, entity_type, entity_id, generated_at, card_score AS score
    FROM qualified_news_summaries WHERE prompt_version IN ('n149-full-profile','n150-card-hook','n151-short-hook') AND id IN (SELECT id FROM news_summaries WHERE scoring_version='distinct-reports-v1') AND nullif(trim(body),'') IS NOT NULL
    UNION ALL
    SELECT 'Influencer', sport, entity_type, entity_id, generated_at, sentiment FROM qualified_vibe_scores
    WHERE prompt_version IN ('vibe-frame-v65-full-profile','vibe-frame-v66-card-hook','vibe-frame-v67-short-hook') AND nullif(trim(prompt),'') IS NOT NULL
    UNION ALL
    SELECT 'Insider', sport, entity_type, entity_id, generated_at, score FROM qualified_insider_scores
    WHERE prompt_version IN ('insider-source-v47-full-profile','insider-source-v48-card-hook','insider-source-v49-short-hook') AND id IN (SELECT id FROM insider_scores WHERE scoring_version='distinct-reports-v1') AND nullif(trim(read),'') IS NOT NULL
    UNION ALL
    SELECT 'Scout', sport, entity_type, entity_id, generated_at, card_score FROM qualified_stat_summaries
    WHERE prompt_version IN ('s98-stat-direction','s99-card-hook','s100-short-hook') AND nullif(trim(body),'') IS NOT NULL
    UNION ALL
    SELECT 'Analyst', sport, entity_type, entity_id, generated_at, (input_components->>'card_score')::numeric FROM qualified_momentum_summaries
    WHERE prompt_version IN ('momentum-s56-card-scores','momentum-s57-card-hook','momentum-s58-short-hook') AND nullif(trim(blurb),'') IS NOT NULL
    UNION ALL
    SELECT 'Oracle', sport, entity_type, entity_id, generated_at, score FROM qualified_sigil_synthesis
    WHERE prompt_version IN ('or44-finished-cards','or45-card-hook','or46-short-hook') AND nullif(trim(reading),'') IS NOT NULL
), recent AS (
    SELECT * FROM cards WHERE generated_at >= '2026-11-21 23:27:53 America/Detroit'::timestamptz
)
SELECT character, count(generated_at) AS published_rows, count(score) AS scored_rows,
       max(generated_at) AT TIME ZONE 'America/Detroit' AS latest_eastern,
       (array_agg(concat_ws('/',sport,entity_type,entity_id) ORDER BY generated_at DESC)
            FILTER (WHERE generated_at IS NOT NULL))[1] AS latest_target
FROM unnest(ARRAY['Journalist','Influencer','Insider','Scout','Analyst','Oracle']) AS voices(character)
LEFT JOIN recent USING (character) GROUP BY character ORDER BY character;

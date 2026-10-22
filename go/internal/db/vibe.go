package db

// Latest saved daily card, or one explicit Eastern reporting date.
// $1 sport · $2 entity_type · $3 entity_id · $4 date
const entityVibeStatement = `WITH req AS (
	SELECT upper($1::text) AS sport, lower($2::text) AS entity_type, $3::int AS entity_id, $4::date AS reporting_date
),
vibe_cur AS (
	SELECT vs.id, vs.sentiment, vs.hook AS headline, vs.prompt AS body,
	       vs.input_news_ids, vs.source_references,
	       vs.trigger_type, vs.generated_at,
	       vs.model_version, vs.prompt_version, vs.input_hash, vs.scoring_version,
           vs.reporting_date, vs.reporting_start, vs.reporting_end, vs.evidence_cutoff
	FROM public.qualified_vibe_scores vs, req
	WHERE vs.entity_type = req.entity_type
	  AND vs.entity_id = req.entity_id
	  AND vs.sport = req.sport
      AND (req.reporting_date IS NULL OR vs.reporting_date=req.reporting_date)
	  AND NULLIF(btrim(vs.hook), '') IS NOT NULL
	  AND char_length(vs.hook) <= 140
	  AND NULLIF(btrim(vs.prompt), '') IS NOT NULL
	ORDER BY vs.reporting_date DESC, vs.generated_at DESC, vs.id DESC
	LIMIT 1
),
vibe_window AS (
	SELECT DISTINCT ON(vs.reporting_date) vs.reporting_date,vs.sentiment, vs.generated_at, vs.trigger_type,
	       vs.hook AS headline, vs.prompt AS body
	FROM public.qualified_vibe_scores vs, req
	WHERE vs.entity_type = req.entity_type
	  AND vs.entity_id = req.entity_id
	  AND vs.sport = req.sport
      AND (req.reporting_date IS NULL OR vs.reporting_date=req.reporting_date)
	  AND vs.sentiment IS NOT NULL
      AND NULLIF(btrim(vs.hook), '') IS NOT NULL AND char_length(vs.hook) <= 140
      AND NULLIF(btrim(vs.prompt), '') IS NOT NULL
	  AND (req.reporting_date IS NOT NULL OR vs.reporting_date >= (NOW() AT TIME ZONE 'America/Detroit')::date-21)
	ORDER BY vs.reporting_date DESC,vs.generated_at DESC,vs.id DESC
)
SELECT json_build_object(
	'page', 'vibe',
	'sport', lower((SELECT sport FROM req)),
	'entity_type', (SELECT entity_type FROM req),
	'entity_id', (SELECT entity_id FROM req),
	'current', (
		SELECT row_to_json(v) FROM (
			SELECT id, sentiment AS heat, sentiment AS score, headline, body,
			       trigger_type, generated_at, model_version, prompt_version,
                       input_hash, scoring_version AS score_scale, reporting_date AS date,
                       reporting_start, reporting_end, evidence_cutoff,
			       COALESCE(source_references::json, (SELECT json_agg(json_build_object(
			           'id', a.id, 'publisher', a.source, 'url', a.url,
			           'published_at', a.published_at
			       ) ORDER BY a.id) FROM public.news_articles a
			       WHERE a.id = ANY(vibe_cur.input_news_ids)), '[]'::json) AS sources
			FROM vibe_cur
		) v
	),
	'date', COALESCE((SELECT reporting_date FROM req),(SELECT reporting_date FROM vibe_cur)),
	'snapshots', COALESCE(
		(SELECT json_agg(json_build_object(
			'date',reporting_date,'sentiment', sentiment,
			'generated_at', generated_at,
			'trigger_type', trigger_type,
			'headline', headline,
			'body', body
		) ORDER BY generated_at DESC) FROM vibe_window),
		'[]'::json
	)
)`

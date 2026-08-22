package db

// The Vibe card — The Influencer's per-entity product, restored to its own route.
//
// Every other voice has a per-entity endpoint: the Scout at /rating, the Analyst
// at /momentum, the Journalist at /news, the Insider at /transfers, the Oracle at
// /sigil. The Influencer lost hers in the O14 convergence rename, which reused
// the per-entity /vibes path for the Oracle's /sigil and never gave the Vibe card
// a replacement door. Her output kept flowing the whole time — vibe_scores is
// written on every milestone, periodic and news-spike trigger — but the only way
// to read one entity's card was to fetch the *Analyst's* /momentum payload and
// dig into vibes.snapshots[0]. A product riding inside another character's
// endpoint is a product with no contract of its own.
//
// This statement gives it one, in the Drop 2 voice-card shape the other five
// serve: {headline, body} plus the score. The column names differ because hers
// predate the contract — `hook` is the headline (mig 180, v13; NULL on older
// rows) and `prompt` is the body, the same felt-read blurb the vibes leaderboard
// serves AS blurb.
//
// Serve-latest, matching entity_sigil (Scott, 2026-07-16): `current` is the
// latest row carrying a real sentiment at ANY age, timestamped client-side
// rather than hidden behind a freshness window. Only an entity never scored at
// all serves current: null — and it serves 200 with that null, never a 404, so
// the card renders its own empty state exactly as the momentum vibes panel does.
//
// `snapshots` is the 7-day window, the same slice /momentum exposes, kept here
// so the card is self-sufficient and a client rendering it needs one request
// rather than two. The season-length sparkline deliberately stays on /momentum:
// it is a trajectory across the Rating × Vibe axes and belongs with the Analyst.
//
// $1 sport · $2 entity_type · $3 entity_id
const entityVibeStatement = `WITH req AS (
	SELECT upper($1::text) AS sport, lower($2::text) AS entity_type, $3::int AS entity_id
),
vibe_cur AS (
	-- Legacy blurb-only rows (sentiment IS NULL) are pre-scoring scaffolding and
	-- are excluded, consistent with every other reader of this table.
	SELECT vs.sentiment, vs.hook AS headline, vs.prompt AS body,
	       vs.trigger_type, vs.generated_at,
	       vs.model_version, vs.prompt_version
	FROM public.vibe_scores vs, req
	WHERE vs.entity_type = req.entity_type
	  AND vs.entity_id = req.entity_id
	  AND vs.sport = req.sport
	  AND vs.sentiment IS NOT NULL
	ORDER BY vs.generated_at DESC
	LIMIT 1
),
vibe_window AS (
	SELECT vs.sentiment, vs.generated_at, vs.trigger_type
	FROM public.vibe_scores vs, req
	WHERE vs.entity_type = req.entity_type
	  AND vs.entity_id = req.entity_id
	  AND vs.sport = req.sport
	  AND vs.sentiment IS NOT NULL
	  AND vs.generated_at >= NOW() - INTERVAL '7 days'
	ORDER BY vs.generated_at DESC
)
SELECT json_build_object(
	'page', 'vibe',
	'sport', lower((SELECT sport FROM req)),
	'entity_type', (SELECT entity_type FROM req),
	'entity_id', (SELECT entity_id FROM req),
	'current', (
		SELECT row_to_json(v) FROM (
			-- heat mirrors entity_sigil: the number this card ranks by, named
			-- the same way across every surface that carries one.
			SELECT sentiment, sentiment AS heat, headline, body,
			       trigger_type, generated_at, model_version, prompt_version
			FROM vibe_cur
		) v
	),
	'window_days', 7,
	'snapshots', COALESCE(
		(SELECT json_agg(json_build_object(
			'sentiment', sentiment,
			'generated_at', generated_at,
			'trigger_type', trigger_type
		) ORDER BY generated_at DESC) FROM vibe_window),
		'[]'::json
	)
)`

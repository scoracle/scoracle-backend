-- Evidence from the current source revision, not the character's generated story.
WITH eligible AS (
    SELECT x.article_id, x.clean_text AS headline, s.source->>'source' AS publisher,
           a.published_at, public.headline_content_key(x.clean_text) AS content_key
    FROM classifier_spectra x
    JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id AND d.character='journalist'
    JOIN classifier_source_heads h ON (h.sport,h.article_id,h.source_id)=(x.sport,x.article_id,x.source_id)
    JOIN classifier_sources s ON s.id=h.source_id
    JOIN news_articles a ON a.id=x.article_id
    WHERE x.sport=upper($1::text) AND x.target->>'entity_type'=$2::text
      AND x.target->>'entity_id'=($3::int)::text
      AND d.status IN ('pending','used')
      AND s.source->>'evidence_scope'='headline'
      AND s.source->'identity_candidates' @> jsonb_build_array(x.target)
      AND a.duplicate_of IS NULL AND a.title=x.clean_text AND btrim(x.clean_text)<>''
      AND btrim(s.source->>'source')<>'' AND a.published_at<=now()
      AND a.published_at=(s.source->>'published_at')::timestamptz
      AND ($4::date IS NULL OR (a.published_at AT TIME ZONE 'America/Detroit')::date=$4)
), edition AS (
    SELECT coalesce($4::date,(max(published_at) AT TIME ZONE 'America/Detroit')::date) AS day FROM eligible
), unique_headlines AS (
    SELECT DISTINCT ON (content_key) article_id, headline, publisher, published_at
    FROM eligible, edition WHERE (published_at AT TIME ZONE 'America/Detroit')::date=day
    ORDER BY content_key, published_at DESC, article_id DESC
), selected AS (
    SELECT * FROM unique_headlines ORDER BY published_at DESC, article_id DESC LIMIT 5
)
SELECT json_build_object('page','articulator','sport',lower($1),'entity_type',$2,
    'entity_id',$3,'date',(SELECT day FROM edition),'headlines',coalesce((
        SELECT json_agg(selected ORDER BY published_at DESC,article_id DESC) FROM selected
    ),'[]'::json))

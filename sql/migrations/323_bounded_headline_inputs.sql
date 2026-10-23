BEGIN;

CREATE TABLE public.headline_fetch_watermarks (
    entity_type text NOT NULL,
    entity_id bigint NOT NULL,
    sport text NOT NULL,
    fetched_through timestamptz NOT NULL,
    recheck_after timestamptz,
    PRIMARY KEY (entity_type,entity_id,sport)
);

-- Normalize presentation only: keep numbers, negation, punctuation and word order.
CREATE FUNCTION public.headline_content_key(headline text) RETURNS text
LANGUAGE sql IMMUTABLE STRICT PARALLEL SAFE AS $$
    SELECT lower(btrim(regexp_replace(translate(normalize(headline,NFKC),
        '‘’“”–—','''''""--'), '[[:space:]]+', ' ', 'g')))
$$;
-- Hash only the index key: archived text can exceed a B-tree entry; lookups also compare full text.
CREATE INDEX news_articles_headline_content ON public.news_articles
    (md5(public.headline_content_key(title)),id) WHERE duplicate_of IS NULL;
CREATE INDEX classifier_measurement_request_reuse ON public.classifier_measurements
    (sport,request_hash,id DESC) WHERE status='source_bound_provisional';
CREATE INDEX classifier_spectra_reviewed_headline ON public.classifier_spectra
    (sport,(target->>'entity_type'),(target->>'entity_id'),md5(public.headline_content_key(clean_text)));

CREATE FUNCTION public.headline_entity_cap() RETURNS integer
LANGUAGE sql IMMUTABLE PARALLEL SAFE AS $$ SELECT 40 $$;

CREATE FUNCTION public.coverage_select_headlines(spam_spectra bigint[] DEFAULT '{}'::bigint[], assignment_scope jsonb DEFAULT NULL) RETURNS bigint LANGUAGE plpgsql AS $$
DECLARE changed bigint;
BEGIN
    PERFORM pg_advisory_xact_lock(1935896421,1);
    IF NOT EXISTS(SELECT 1 FROM coverage_schedule WHERE enabled AND phase='characters') THEN
        RAISE EXCEPTION 'Headline selection requires a frozen character edition';
    END IF;
    IF EXISTS(SELECT 1 FROM pipeline_work WHERE coverage_is_character(stage) AND status='running') THEN
        RAISE EXCEPTION 'Headline selection requires character workers to be idle';
    END IF;
    WITH eligible AS MATERIALIZED (
        SELECT d.spectrum_id,d.character,a.spectrum_ids,x.sport,x.target,x.spectrum,
            x.id=ANY(spam_spectra) AS spam,
            public.headline_content_key(x.clean_text) AS content_key,
            (s.source->>'published_at')::timestamptz AS published_at,
            CASE d.character WHEN 'journalist' THEN 'subject' WHEN 'influencer' THEN 'emotion'
                WHEN 'insider' THEN 'movement' WHEN 'scout' THEN 'availability' END AS criterion
        FROM coverage_assignments a JOIN coverage_schedule c USING(edition_date)
        JOIN classifier_spectra x ON x.id=ANY(a.spectrum_ids)
        JOIN classifier_sources s ON s.id=x.source_id
        JOIN classifier_spectrum_deliveries d ON d.spectrum_id=x.id
            AND d.character=CASE a.stage WHEN 'narratives' THEN 'journalist' WHEN 'vibe' THEN 'influencer'
                WHEN 'transfers' THEN 'insider' WHEN 'rating' THEN 'scout' END
        WHERE c.enabled AND c.phase='characters' AND d.status='pending'
            AND (assignment_scope IS NULL OR assignment_scope @> jsonb_build_array(jsonb_build_object(
                'stage',a.stage,'entity_type',a.entity_type,'entity_id',a.entity_id,'sport',a.sport)))
            AND EXISTS(SELECT 1 FROM jsonb_array_elements(s.source->'identity_candidates') candidate
                WHERE candidate->>'entity_type'=a.entity_type AND candidate->>'entity_id'=a.entity_id::text)
    ), copies AS MATERIALIZED (
        SELECT e.*, first_value(spectrum_id) OVER w AS canonical_spectrum,
            row_number() OVER w AS copy_number,
            EXISTS(SELECT 1 FROM classifier_spectra previous
                JOIN classifier_spectrum_deliveries receipt ON receipt.spectrum_id=previous.id
                WHERE previous.sport=e.sport AND previous.target->>'entity_type'=e.target->>'entity_type'
                    AND previous.target->>'entity_id'=e.target->>'entity_id'
                    AND md5(public.headline_content_key(previous.clean_text))=md5(e.content_key)
                    AND public.headline_content_key(previous.clean_text)=e.content_key
                    AND receipt.character=e.character AND receipt.status IN ('used','abstained')
                    AND receipt.reason IS DISTINCT FROM 'ranked_below_entity_cap'
                    AND NOT(previous.id=ANY(e.spectrum_ids))) AS reviewed
        FROM eligible e
        WINDOW w AS (PARTITION BY character,sport,target,content_key
            ORDER BY (spectrum->>criterion)::numeric DESC NULLS LAST,
                (spectrum->>'subject')::numeric DESC NULLS LAST,published_at DESC NULLS LAST,spectrum_id DESC)
    ), ranked AS (
        SELECT *,row_number() OVER (PARTITION BY character,sport,target
            ORDER BY (spectrum->>criterion)::numeric DESC NULLS LAST,
                (spectrum->>'subject')::numeric DESC NULLS LAST,published_at DESC NULLS LAST,spectrum_id DESC) AS priority
        FROM copies WHERE copy_number=1 AND NOT reviewed AND NOT spam
    ), dispositions AS (
        SELECT spectrum_id,character,'redundant' AS status,
            CASE WHEN reviewed THEN 'already_reviewed_headline' ELSE 'reposted_headline' END AS reason,
            jsonb_build_object('canonical_spectrum_id',canonical_spectrum) AS detail
        FROM copies WHERE NOT spam AND (reviewed OR copy_number>1)
        UNION ALL
        SELECT spectrum_id,character,'abstained','livestream_spam',
            jsonb_build_object('rule','stream-solicitation-v2') FROM copies WHERE spam
        UNION ALL
        SELECT spectrum_id,character,'abstained','ranked_below_entity_cap',
            jsonb_build_object('rank',priority,'criterion',criterion,'scores',spectrum)
        FROM ranked WHERE priority>public.headline_entity_cap()
    ) UPDATE classifier_spectrum_deliveries d SET status=p.status,reason=p.reason,
        product_ref=jsonb_build_object('selection_policy','headline-selection-v2','entity_cap',public.headline_entity_cap(),
            'model_called',false,'selection',p.detail),updated_at=clock_timestamp()
        FROM dispositions p WHERE d.spectrum_id=p.spectrum_id AND d.character=p.character AND d.status='pending';
    GET DIAGNOSTICS changed=ROW_COUNT;
    RETURN changed;
END $$;

INSERT INTO public.schema_migrations(version) VALUES ('323_bounded_headline_inputs') ON CONFLICT DO NOTHING;
COMMIT;

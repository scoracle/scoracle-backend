-- Let settled source facts nominate current-team reconciliation after rumor heat has decayed.
-- The existing adjudicator and application/override ledgers still own the decision and write.
-- A current-season stats row at the proposed club plus two independently named, Editor-linked
-- roster/performance/transfer sources is sufficient to ASK for adjudication; it does not itself
-- apply the move. The real heat remains in the audit row and is never raised to mimic eligibility.
BEGIN;

ALTER TABLE public.transfer_identity_applications
    ADD COLUMN IF NOT EXISTS evidence_route text NOT NULL DEFAULT 'rumor_threshold'
        CHECK (evidence_route IN ('rumor_threshold', 'settled_sources'));

COMMENT ON COLUMN public.transfer_identity_applications.evidence_route IS
    'Deterministic nomination route. rumor_threshold passed the configured heat gate; '
    'settled_sources passed settled_transfer_identity_evidence. Both still require the '
    'same fail-closed identity adjudication and fixed entity IDs.';

CREATE OR REPLACE FUNCTION public.settled_transfer_identity_evidence(
    p_sport text,
    p_player_id integer,
    p_team_id integer,
    p_news_ids bigint[]
)
RETURNS TABLE (
    eligible boolean,
    stats_season integer,
    article_ids bigint[],
    source_names text[]
)
LANGUAGE sql STABLE
AS $$
WITH latest AS (
    SELECT max(ps.season)::integer AS season
    FROM public.player_stats ps
    WHERE ps.sport = p_sport AND ps.player_id = p_player_id
), same_team_stats AS (
    SELECT ps.season
    FROM public.player_stats ps
    JOIN latest l ON l.season = ps.season
    WHERE ps.sport = p_sport
      AND ps.player_id = p_player_id
      AND ps.team_id = p_team_id
    LIMIT 1
), linked_news AS (
    SELECT DISTINCT n.id, nullif(btrim(n.source), '') AS source
    FROM public.news_articles n
    JOIN public.editor_reads er ON er.article_id = n.id AND er.status = 'success'
    WHERE n.id = ANY(COALESCE(p_news_ids, ARRAY[]::bigint[]))
      AND er.read->>'story_type' IN ('roster', 'performance', 'transfer')
      AND EXISTS (
          SELECT 1
          FROM jsonb_array_elements(COALESCE(er.resolved->'links', '[]'::jsonb)) link
          WHERE link->>'sport' = p_sport
            AND link->>'entity_type' = 'player'
            AND link->>'entity_id' = p_player_id::text
      )
      AND EXISTS (
          SELECT 1
          FROM jsonb_array_elements(COALESCE(er.resolved->'links', '[]'::jsonb)) link
          WHERE link->>'sport' = p_sport
            AND link->>'entity_type' = 'team'
            AND link->>'entity_id' = p_team_id::text
      )
), evidence AS (
    SELECT count(DISTINCT lower(source)) FILTER (WHERE source IS NOT NULL) AS source_count,
           COALESCE(array_agg(id ORDER BY id), ARRAY[]::bigint[]) AS article_ids,
           COALESCE(array_agg(DISTINCT source ORDER BY source)
                    FILTER (WHERE source IS NOT NULL), ARRAY[]::text[]) AS source_names
    FROM linked_news
)
SELECT EXISTS (SELECT 1 FROM same_team_stats) AND e.source_count >= 2,
       (SELECT season FROM same_team_stats),
       e.article_ids,
       e.source_names
FROM evidence e;
$$;

COMMENT ON FUNCTION public.settled_transfer_identity_evidence(text, integer, integer, bigint[]) IS
    'Read-only nomination gate for current-team reconciliation after rumor heat decays: the '
    'player latest-season stats must name the proposed team and the supplied pair corpus must '
    'contain exact Editor links from at least two independently named roster, performance, or '
    'transfer sources. Returns the retained article IDs for the identity adjudicator.';

-- Overload the mature application owner with an explicit evidence route. The 12-argument
-- function remains the compatibility path for existing callers and continues to enforce heat.
CREATE OR REPLACE FUNCTION public.apply_transfer_identity_candidate(
    p_sport text,
    p_player_id integer,
    p_old_team_id integer,
    p_new_team_id integer,
    p_source_rumor_id bigint,
    p_source_synthesis_id bigint,
    p_deterministic_heat smallint,
    p_deterministic_confidence numeric,
    p_adjudication jsonb,
    p_adjudication_raw text,
    p_adjudication_model_version text,
    p_adjudication_prompt_version text,
    p_evidence_route text,
    p_evidence_news_ids bigint[]
)
RETURNS TABLE (application_id bigint, override_id bigint, status text, reason text)
LANGUAGE plpgsql
AS $$
DECLARE
    v_threshold public.transfer_identity_thresholds%ROWTYPE;
    v_eligible boolean;
    v_stats_season integer;
    v_article_ids bigint[];
    v_source_names text[];
    v_application_id bigint;
    v_override_id bigint;
    v_status text;
    v_reason text;
    v_gate_heat smallint := p_deterministic_heat;
    v_gate_confidence numeric := p_deterministic_confidence;
BEGIN
    IF p_evidence_route NOT IN ('rumor_threshold', 'settled_sources') THEN
        RAISE EXCEPTION 'unsupported transfer identity evidence route: %', p_evidence_route;
    END IF;

    IF p_evidence_route = 'settled_sources' THEN
        SELECT e.eligible, e.stats_season, e.article_ids, e.source_names
          INTO v_eligible, v_stats_season, v_article_ids, v_source_names
          FROM public.settled_transfer_identity_evidence(
              p_sport, p_player_id, p_new_team_id, p_evidence_news_ids) e;
        IF NOT COALESCE(v_eligible, false) THEN
            RAISE EXCEPTION 'settled transfer identity evidence no longer qualifies for %.% -> %',
                p_sport, p_player_id, p_new_team_id;
        END IF;

        SELECT * INTO v_threshold
        FROM public.transfer_identity_thresholds
        WHERE sport = p_sport;
        IF NOT FOUND THEN
            RAISE EXCEPTION 'missing transfer identity threshold config for sport %', p_sport;
        END IF;

        -- The underlying owner remains unchanged. These gate-only values let it run its mature
        -- ID/adjudication/current-state checks; the application is immediately restored to the
        -- actual heat/confidence below and records why the heat gate was not used.
        v_gate_heat := v_threshold.min_heat;
        v_gate_confidence := v_threshold.min_deterministic_confidence;
    ELSE
        v_article_ids := COALESCE(p_evidence_news_ids, ARRAY[]::bigint[]);
        v_source_names := ARRAY[]::text[];
    END IF;

    SELECT r.application_id, r.override_id, r.status, r.reason
      INTO v_application_id, v_override_id, v_status, v_reason
      FROM public.apply_transfer_identity_candidate(
          p_sport, p_player_id, p_old_team_id, p_new_team_id,
          p_source_rumor_id, p_source_synthesis_id,
          v_gate_heat, v_gate_confidence,
          p_adjudication, p_adjudication_raw,
          p_adjudication_model_version, p_adjudication_prompt_version) r;

    UPDATE public.transfer_identity_applications a
       SET deterministic_heat = p_deterministic_heat,
           deterministic_confidence = p_deterministic_confidence,
           evidence_route = p_evidence_route,
           threshold_config = a.threshold_config || jsonb_build_object(
               'evidence_route', p_evidence_route),
           evidence = a.evidence || jsonb_build_object(
               'identity_evidence_route', p_evidence_route,
               'identity_evidence_article_ids', COALESCE(v_article_ids, ARRAY[]::bigint[]),
               'identity_evidence_sources', COALESCE(v_source_names, ARRAY[]::text[]),
               'identity_evidence_stats_season', v_stats_season)
     WHERE a.id = v_application_id;

    IF v_override_id IS NOT NULL THEN
        UPDATE public.player_current_identity_overrides o
           SET evidence = o.evidence || jsonb_build_object(
               'identity_evidence_route', p_evidence_route,
               'identity_evidence_article_ids', COALESCE(v_article_ids, ARRAY[]::bigint[]),
               'identity_evidence_sources', COALESCE(v_source_names, ARRAY[]::text[]),
               'identity_evidence_stats_season', v_stats_season)
         WHERE o.id = v_override_id;
    END IF;

    RETURN QUERY SELECT v_application_id, v_override_id, v_status, v_reason;
END;
$$;

COMMENT ON FUNCTION public.apply_transfer_identity_candidate(
    text, integer, integer, integer, bigint, bigint, smallint, numeric,
    jsonb, text, text, text, text, bigint[]) IS
    'Evidence-route overload for the existing transfer identity owner. rumor_threshold keeps '
    'the configured heat gate. settled_sources revalidates current-season stats plus two exact '
    'Editor-linked sources, then uses the same adjudication/current-ID/apply workflow while '
    'retaining the actual decayed heat in the audit row.';

INSERT INTO public.schema_migrations(version)
VALUES ('254_settled_transfer_identity_evidence')
ON CONFLICT (version) DO NOTHING;

COMMIT;

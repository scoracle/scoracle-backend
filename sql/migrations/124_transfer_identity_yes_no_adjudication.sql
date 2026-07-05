-- 124_transfer_identity_yes_no_adjudication.sql
--
-- Keep the transfer identity workflow as a two-step gate:
--   1. deterministic heat decides whether a candidate deserves model adjudication;
--   2. the local model returns a yes/no decision against the evidence articles and fixed entity IDs.
--
-- Model self-confidence is retained only as optional audit data. It must not be a hidden third gate
-- after the model has already said "apply".

BEGIN;

CREATE OR REPLACE FUNCTION public.apply_transfer_identity_candidate(
    p_sport TEXT,
    p_player_id INTEGER,
    p_old_team_id INTEGER,
    p_new_team_id INTEGER,
    p_source_rumor_id BIGINT,
    p_source_synthesis_id BIGINT,
    p_deterministic_heat SMALLINT,
    p_deterministic_confidence NUMERIC,
    p_adjudication JSONB,
    p_adjudication_raw TEXT,
    p_adjudication_model_version TEXT,
    p_adjudication_prompt_version TEXT
)
RETURNS TABLE (application_id BIGINT, override_id BIGINT, status TEXT, reason TEXT) AS $$
DECLARE
    v_threshold public.transfer_identity_thresholds%ROWTYPE;
    v_current_team_id INTEGER;
    v_current_league_id INTEGER;
    v_new_league_id INTEGER;
    v_decision TEXT;
    v_event_type TEXT;
    v_conf NUMERIC;
    v_reason TEXT;
    v_adj_old_team_id INTEGER;
    v_adj_new_team_id INTEGER;
    v_status TEXT;
    v_existing BIGINT;
    v_app_id BIGINT;
    v_override_id BIGINT;
    v_threshold_json JSONB;
BEGIN
    SELECT * INTO v_threshold
    FROM public.transfer_identity_thresholds
    WHERE sport = p_sport;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'missing transfer identity threshold config for sport %', p_sport;
    END IF;

    SELECT team_id, league_id INTO v_current_team_id, v_current_league_id
    FROM public.player_current_identity
    WHERE sport = p_sport AND player_id = p_player_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'player %.% not found in player_current_identity', p_sport, p_player_id;
    END IF;

    SELECT league_id INTO v_new_league_id
    FROM public.teams
    WHERE sport = p_sport AND id = p_new_team_id;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'new team %.% not found', p_sport, p_new_team_id;
    END IF;

    v_threshold_json := to_jsonb(v_threshold);
    v_decision := p_adjudication->>'decision';
    v_event_type := p_adjudication->>'event_type';
    v_conf := NULLIF(p_adjudication->>'confidence', '')::numeric;
    v_reason := NULLIF(p_adjudication->>'reason', '');
    v_adj_old_team_id := NULLIF(p_adjudication->>'old_team_id', '')::integer;
    v_adj_new_team_id := NULLIF(p_adjudication->>'new_team_id', '')::integer;

    IF p_deterministic_heat < v_threshold.min_heat
       OR p_deterministic_confidence < v_threshold.min_deterministic_confidence THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'deterministic threshold not met');
    ELSIF v_decision = 'reject' THEN
        v_status := 'rejected';
        v_reason := COALESCE(v_reason, 'adjudicator rejected candidate');
    ELSIF v_decision <> 'apply' THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'invalid adjudication decision');
    ELSIF v_event_type IS NULL OR NOT (v_event_type = ANY(v_threshold.allowed_event_types)) THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'unsupported adjudication event_type');
    ELSIF v_adj_new_team_id IS DISTINCT FROM p_new_team_id
       OR v_adj_old_team_id IS DISTINCT FROM p_old_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'adjudication team IDs conflict with deterministic candidate');
    ELSIF v_current_team_id IS DISTINCT FROM p_old_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'current identity changed before apply');
    ELSIF p_old_team_id IS NOT NULL AND p_old_team_id = p_new_team_id THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'candidate destination is already current team');
    ELSE
        v_status := 'applied';
        v_reason := COALESCE(v_reason, 'adjudicator approved current identity update');
    END IF;

    IF v_status = 'applied' THEN
        SELECT a_existing.id INTO v_existing
        FROM public.transfer_identity_applications a_existing
        WHERE a_existing.sport = p_sport
          AND a_existing.player_id = p_player_id
          AND COALESCE(a_existing.old_team_id, 0) = COALESCE(p_old_team_id, 0)
          AND a_existing.new_team_id = p_new_team_id
          AND COALESCE(a_existing.source_rumor_id, 0) = COALESCE(p_source_rumor_id, 0)
          AND COALESCE(a_existing.source_synthesis_id, 0) = COALESCE(p_source_synthesis_id, 0)
          AND a_existing.status IN ('applied', 'reverted')
        ORDER BY a_existing.created_at DESC
        LIMIT 1;

        IF v_existing IS NOT NULL THEN
            RETURN QUERY
            SELECT a.id, a.override_id, a.status, 'idempotent: transition already recorded'::text
            FROM public.transfer_identity_applications a
            WHERE a.id = v_existing;
            RETURN;
        END IF;
    END IF;

    INSERT INTO public.transfer_identity_applications (
        sport, player_id, old_team_id, old_league_id, new_team_id, new_league_id,
        source_rumor_id, source_synthesis_id, deterministic_heat, deterministic_confidence,
        threshold_config, adjudication, adjudication_raw, adjudication_model_version,
        adjudication_prompt_version, decision, event_type, adjudication_confidence,
        status, reason, evidence, applied_at
    ) VALUES (
        p_sport, p_player_id, p_old_team_id, v_current_league_id, p_new_team_id, v_new_league_id,
        p_source_rumor_id, p_source_synthesis_id, p_deterministic_heat, p_deterministic_confidence,
        v_threshold_json, COALESCE(p_adjudication, '{}'::jsonb), p_adjudication_raw,
        p_adjudication_model_version, p_adjudication_prompt_version, COALESCE(v_decision, 'failed_closed'),
        v_event_type, v_conf, v_status, v_reason,
        jsonb_build_object('source', 'mistral_adjudication', 'raw', p_adjudication_raw),
        CASE WHEN v_status = 'applied' THEN NOW() ELSE NULL END
    )
    RETURNING id INTO v_app_id;

    IF v_status = 'applied' THEN
        INSERT INTO public.player_current_identity_overrides (
            sport, player_id, team_id, league_id, source, source_rumor_id, source_synthesis_id,
            confidence, reason, evidence, applied_by
        ) VALUES (
            p_sport, p_player_id, p_new_team_id, v_new_league_id, 'applied_transfer',
            p_source_rumor_id, p_source_synthesis_id, v_conf, v_reason,
            jsonb_build_object(
                'application_id', v_app_id,
                'old_team_id', p_old_team_id,
                'old_league_id', v_current_league_id,
                'threshold', v_threshold_json,
                'adjudication', COALESCE(p_adjudication, '{}'::jsonb)
            ),
            'transfer_identity_workflow'
        )
        RETURNING id INTO v_override_id;

        UPDATE public.transfer_identity_applications
        SET override_id = v_override_id
        WHERE id = v_app_id;

        PERFORM public.refresh_sport_autofill(p_sport, 'applied_transfer_identity');
    END IF;

    RETURN QUERY SELECT v_app_id, v_override_id, v_status, v_reason;
END;
$$ LANGUAGE plpgsql;

INSERT INTO public.schema_migrations(version)
VALUES ('124_transfer_identity_yes_no_adjudication')
ON CONFLICT (version) DO NOTHING;

COMMIT;

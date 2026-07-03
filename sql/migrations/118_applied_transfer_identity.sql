-- 118_applied_transfer_identity.sql
--
-- Auditable, thresholded current-identity updates from vetted transfer/trade
-- rumors. This deliberately writes only current-identity override state; it
-- never rewrites historical stats, events, fixtures, or roster rows.

BEGIN;

CREATE TABLE IF NOT EXISTS public.transfer_identity_thresholds (
    sport                         TEXT PRIMARY KEY REFERENCES public.sports(id),
    min_heat                      SMALLINT NOT NULL DEFAULT 80 CHECK (min_heat BETWEEN 0 AND 100),
    min_deterministic_confidence  NUMERIC(4,3) NOT NULL DEFAULT 0.800 CHECK (min_deterministic_confidence BETWEEN 0 AND 1),
    min_adjudication_confidence   NUMERIC(4,3) NOT NULL DEFAULT 0.850 CHECK (min_adjudication_confidence BETWEEN 0 AND 1),
    allowed_event_types           TEXT[] NOT NULL DEFAULT ARRAY['transfer','trade','loan','signing']::text[],
    updated_at                    TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

INSERT INTO public.transfer_identity_thresholds (sport)
SELECT id FROM public.sports WHERE id IN ('NBA', 'NFL', 'FOOTBALL')
ON CONFLICT (sport) DO NOTHING;

CREATE TABLE IF NOT EXISTS public.sport_autofill_versions (
    sport          TEXT PRIMARY KEY REFERENCES public.sports(id),
    version        BIGINT NOT NULL DEFAULT 1,
    generated_at   TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    total_entities INTEGER NOT NULL DEFAULT 0,
    status         TEXT NOT NULL DEFAULT 'ready' CHECK (status IN ('ready', 'refreshing', 'failed')),
    reason         TEXT
);

INSERT INTO public.sport_autofill_versions (sport, total_entities)
SELECT s.id,
       CASE s.id
           WHEN 'NBA' THEN COALESCE((SELECT COUNT(*)::int FROM nba.autofill_entities), 0)
           WHEN 'NFL' THEN COALESCE((SELECT COUNT(*)::int FROM nfl.autofill_entities), 0)
           WHEN 'FOOTBALL' THEN COALESCE((SELECT COUNT(*)::int FROM football.autofill_entities), 0)
           ELSE 0
       END
FROM public.sports s
WHERE s.id IN ('NBA', 'NFL', 'FOOTBALL')
ON CONFLICT (sport) DO NOTHING;

CREATE TABLE IF NOT EXISTS public.transfer_identity_applications (
    id                            BIGSERIAL PRIMARY KEY,
    sport                         TEXT NOT NULL REFERENCES public.sports(id),
    player_id                     INTEGER NOT NULL,
    old_team_id                   INTEGER,
    old_league_id                 INTEGER,
    new_team_id                   INTEGER NOT NULL,
    new_league_id                 INTEGER,
    source_rumor_id               BIGINT,
    source_synthesis_id           BIGINT,
    deterministic_heat            SMALLINT NOT NULL CHECK (deterministic_heat BETWEEN 0 AND 100),
    deterministic_confidence      NUMERIC(4,3) NOT NULL CHECK (deterministic_confidence BETWEEN 0 AND 1),
    threshold_config              JSONB NOT NULL DEFAULT '{}'::jsonb,
    adjudication                  JSONB NOT NULL DEFAULT '{}'::jsonb,
    adjudication_raw              TEXT,
    adjudication_model_version    TEXT,
    adjudication_prompt_version   TEXT,
    decision                      TEXT NOT NULL CHECK (decision IN ('apply', 'reject', 'manual_review', 'failed_closed')),
    event_type                    TEXT,
    adjudication_confidence       NUMERIC(4,3) CHECK (adjudication_confidence IS NULL OR adjudication_confidence BETWEEN 0 AND 1),
    status                        TEXT NOT NULL CHECK (status IN ('applied', 'rejected', 'manual_review', 'failed_closed', 'reverted')),
    reason                        TEXT,
    evidence                      JSONB NOT NULL DEFAULT '{}'::jsonb,
    override_id                   BIGINT REFERENCES public.player_current_identity_overrides(id),
    applied_at                    TIMESTAMPTZ,
    reverted_at                   TIMESTAMPTZ,
    reverted_by                   TEXT,
    revert_reason                 TEXT,
    created_at                    TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (player_id, sport) REFERENCES public.players(id, sport) ON DELETE CASCADE,
    FOREIGN KEY (old_team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT,
    FOREIGN KEY (new_team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_transfer_identity_applications_player
    ON public.transfer_identity_applications (sport, player_id, created_at DESC);

CREATE INDEX IF NOT EXISTS idx_transfer_identity_applications_source_rumor
    ON public.transfer_identity_applications (source_rumor_id)
    WHERE source_rumor_id IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_transfer_identity_applications_idempotent_applied
    ON public.transfer_identity_applications (
        sport,
        player_id,
        COALESCE(old_team_id, 0),
        new_team_id,
        COALESCE(source_rumor_id, 0),
        COALESCE(source_synthesis_id, 0)
    )
    WHERE status IN ('applied', 'reverted');

CREATE UNIQUE INDEX IF NOT EXISTS idx_player_current_identity_overrides_transfer_idempotent
    ON public.player_current_identity_overrides (
        sport,
        player_id,
        COALESCE(team_id, 0),
        COALESCE(source_rumor_id, 0),
        COALESCE(source_synthesis_id, 0)
    )
    WHERE source = 'applied_transfer';

CREATE OR REPLACE FUNCTION public.refresh_sport_autofill(p_sport TEXT, p_reason TEXT DEFAULT NULL)
RETURNS VOID AS $$
DECLARE
    v_total INTEGER := 0;
BEGIN
    INSERT INTO public.sport_autofill_versions (sport, status, reason)
    VALUES (p_sport, 'refreshing', p_reason)
    ON CONFLICT (sport) DO UPDATE SET
        status = 'refreshing',
        reason = EXCLUDED.reason,
        generated_at = NOW();

    IF p_sport = 'NBA' THEN
        REFRESH MATERIALIZED VIEW nba.autofill_entities;
        SELECT COUNT(*)::int INTO v_total FROM nba.autofill_entities;
    ELSIF p_sport = 'NFL' THEN
        REFRESH MATERIALIZED VIEW nfl.autofill_entities;
        SELECT COUNT(*)::int INTO v_total FROM nfl.autofill_entities;
    ELSIF p_sport = 'FOOTBALL' THEN
        REFRESH MATERIALIZED VIEW football.autofill_entities;
        SELECT COUNT(*)::int INTO v_total FROM football.autofill_entities;
    ELSE
        RAISE EXCEPTION 'unsupported sport for autofill refresh: %', p_sport;
    END IF;

    INSERT INTO public.sport_autofill_versions (sport, version, generated_at, total_entities, status, reason)
    VALUES (p_sport, 1, NOW(), v_total, 'ready', p_reason)
    ON CONFLICT (sport) DO UPDATE SET
        version = public.sport_autofill_versions.version + 1,
        generated_at = NOW(),
        total_entities = EXCLUDED.total_entities,
        status = 'ready',
        reason = EXCLUDED.reason;
EXCEPTION WHEN OTHERS THEN
    UPDATE public.sport_autofill_versions
    SET status = 'failed', generated_at = NOW(), reason = SQLERRM
    WHERE sport = p_sport;
    RAISE;
END;
$$ LANGUAGE plpgsql;

CREATE OR REPLACE FUNCTION public.record_transfer_identity_adjudication_failure(
    p_sport TEXT,
    p_player_id INTEGER,
    p_old_team_id INTEGER,
    p_new_team_id INTEGER,
    p_source_rumor_id BIGINT,
    p_source_synthesis_id BIGINT,
    p_deterministic_heat SMALLINT,
    p_deterministic_confidence NUMERIC,
    p_adjudication_raw TEXT,
    p_adjudication_model_version TEXT,
    p_adjudication_prompt_version TEXT,
    p_reason TEXT
)
RETURNS BIGINT AS $$
DECLARE
    v_old_league_id INTEGER;
    v_new_league_id INTEGER;
    v_threshold public.transfer_identity_thresholds%ROWTYPE;
    v_id BIGINT;
BEGIN
    SELECT * INTO v_threshold
    FROM public.transfer_identity_thresholds
    WHERE sport = p_sport;

    SELECT league_id INTO v_old_league_id
    FROM public.player_current_identity
    WHERE sport = p_sport AND player_id = p_player_id;

    SELECT league_id INTO v_new_league_id
    FROM public.teams
    WHERE sport = p_sport AND id = p_new_team_id;

    INSERT INTO public.transfer_identity_applications (
        sport, player_id, old_team_id, old_league_id, new_team_id, new_league_id,
        source_rumor_id, source_synthesis_id, deterministic_heat, deterministic_confidence,
        threshold_config, adjudication_raw, adjudication_model_version, adjudication_prompt_version,
        decision, status, reason
    ) VALUES (
        p_sport, p_player_id, p_old_team_id, v_old_league_id, p_new_team_id, v_new_league_id,
        p_source_rumor_id, p_source_synthesis_id, p_deterministic_heat, p_deterministic_confidence,
        COALESCE(to_jsonb(v_threshold), '{}'::jsonb), p_adjudication_raw,
        p_adjudication_model_version, p_adjudication_prompt_version,
        'failed_closed', 'failed_closed', p_reason
    )
    RETURNING id INTO v_id;

    RETURN v_id;
END;
$$ LANGUAGE plpgsql;

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
    ELSIF v_decision = 'manual_review' THEN
        v_status := 'manual_review';
        v_reason := COALESCE(v_reason, 'adjudicator requested manual review');
    ELSIF v_decision <> 'apply' THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'invalid adjudication decision');
    ELSIF v_event_type IS NULL OR NOT (v_event_type = ANY(v_threshold.allowed_event_types)) THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'unsupported adjudication event_type');
    ELSIF v_conf IS NULL OR v_conf < v_threshold.min_adjudication_confidence THEN
        v_status := 'failed_closed';
        v_reason := COALESCE(v_reason, 'adjudication confidence below threshold');
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
        SELECT id INTO v_existing
        FROM public.transfer_identity_applications
        WHERE sport = p_sport
          AND player_id = p_player_id
          AND COALESCE(old_team_id, 0) = COALESCE(p_old_team_id, 0)
          AND new_team_id = p_new_team_id
          AND COALESCE(source_rumor_id, 0) = COALESCE(p_source_rumor_id, 0)
          AND COALESCE(source_synthesis_id, 0) = COALESCE(p_source_synthesis_id, 0)
          AND status IN ('applied', 'reverted')
        ORDER BY created_at DESC
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

CREATE OR REPLACE FUNCTION public.revert_applied_transfer_identity(
    p_application_id BIGINT,
    p_reverted_by TEXT,
    p_revert_reason TEXT
)
RETURNS TABLE (application_id BIGINT, override_id BIGINT, status TEXT, reason TEXT) AS $$
DECLARE
    v_app public.transfer_identity_applications%ROWTYPE;
BEGIN
    SELECT * INTO v_app
    FROM public.transfer_identity_applications
    WHERE id = p_application_id
    FOR UPDATE;

    IF NOT FOUND THEN
        RAISE EXCEPTION 'transfer identity application % not found', p_application_id;
    END IF;

    IF v_app.status <> 'applied' OR v_app.override_id IS NULL THEN
        RETURN QUERY SELECT v_app.id, v_app.override_id, v_app.status, 'application is not an active applied override'::text;
        RETURN;
    END IF;

    UPDATE public.player_current_identity_overrides
    SET reverted_at = NOW(),
        reverted_by = p_reverted_by,
        revert_reason = p_revert_reason
    WHERE id = v_app.override_id
      AND reverted_at IS NULL;

    UPDATE public.transfer_identity_applications
    SET status = 'reverted',
        reverted_at = NOW(),
        reverted_by = p_reverted_by,
        revert_reason = p_revert_reason
    WHERE id = p_application_id;

    PERFORM public.refresh_sport_autofill(v_app.sport, 'revert_applied_transfer_identity');

    RETURN QUERY SELECT v_app.id, v_app.override_id, 'reverted'::text, COALESCE(p_revert_reason, 'reverted applied transfer identity');
END;
$$ LANGUAGE plpgsql;

COMMIT;

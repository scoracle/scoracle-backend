-- 189_fixture_boxscore_fetches.sql
--
-- Fixture Boxscore v1: Rust-owned, fixture-keyed data-fetching stage for completed
-- games. This is intentionally a normalized fetch-state table, not a canonical
-- stats writer: event_box_scores/event_team_stats remain owned by the existing
-- provider seeder/finalize path until provider-label mappings are promoted
-- deliberately.

BEGIN;

CREATE TABLE IF NOT EXISTS public.data_fetch_ledger (
    id                      bigserial PRIMARY KEY,
    target_type             text NOT NULL,
    target_id               bigint NOT NULL,
    sport                   text,
    stage                   text NOT NULL,
    status                  text NOT NULL,
    source_url              text,
    final_url               text,
    final_domain            text,
    content_hash            text,
    model_version           text,
    prompt_version          text,
    output_contract_version text,
    parser_outcome          text,
    error                   text,
    generated_at            timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS idx_data_fetch_ledger_target
    ON public.data_fetch_ledger(target_type, target_id, generated_at DESC);

CREATE INDEX IF NOT EXISTS idx_data_fetch_ledger_stage_status
    ON public.data_fetch_ledger(stage, status, generated_at DESC);

CREATE TABLE IF NOT EXISTS public.fixture_boxscore_fetches (
    fixture_id              integer PRIMARY KEY REFERENCES public.fixtures(id) ON DELETE CASCADE,
    sport                   text NOT NULL REFERENCES public.sports(id),
    provider                text NOT NULL,
    source_url              text,
    final_url               text,
    final_domain            text,
    status                  text NOT NULL CHECK (status IN (
        'success',
        'not_supported',
        'not_final',
        'not_found',
        'blocked',
        'fetch_failed',
        'parse_failed',
        'validation_failed'
    )),
    content_hash            text,
    score                   jsonb NOT NULL DEFAULT '{}'::jsonb,
    period_scoring          jsonb NOT NULL DEFAULT '[]'::jsonb,
    team_stats              jsonb NOT NULL DEFAULT '[]'::jsonb,
    player_stats            jsonb NOT NULL DEFAULT '[]'::jsonb,
    raw_labels              jsonb NOT NULL DEFAULT '{}'::jsonb,
    model_version           text,
    prompt_version          text,
    parser_version          text,
    output_contract_version text,
    parser_outcome          text NOT NULL DEFAULT 'no_call',
    last_error              text,
    fetched_at              timestamptz NOT NULL DEFAULT now(),
    updated_at              timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.fixture_boxscore_fetches IS
    'Current normalized box score fetch state per fixture. Written by the Rust '
    'fixture_boxscore stage. This table preserves fetched/provider labels for '
    'downstream recap/stat-commentary work and deliberately does not mutate '
    'canonical event_box_scores/event_team_stats.';

COMMENT ON COLUMN public.fixture_boxscore_fetches.team_stats IS
    'Normalized per-team rows from the selected provider/source. Keys preserve '
    'provider labels when a canonical stat mapping is not yet owned by the seeder.';

COMMENT ON COLUMN public.fixture_boxscore_fetches.player_stats IS
    'Normalized per-player rows from the selected provider/source. Player IDs are '
    'provider IDs unless provider_entity_map resolves them in a later mapping step.';

CREATE INDEX IF NOT EXISTS idx_fixture_boxscore_fetches_status
    ON public.fixture_boxscore_fetches(status, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_fixture_boxscore_fetches_sport_provider
    ON public.fixture_boxscore_fetches(sport, provider, updated_at DESC);

CREATE INDEX IF NOT EXISTS idx_fixture_boxscore_fetches_hash
    ON public.fixture_boxscore_fetches(content_hash)
    WHERE content_hash IS NOT NULL;

ALTER TABLE public.pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_entity_type_check;
ALTER TABLE public.pipeline_work ADD CONSTRAINT pipeline_work_entity_type_check
    CHECK (entity_type IN ('player', 'team', 'article', 'fixture'));

CREATE OR REPLACE FUNCTION public.fixture_boxscore_input_version(p_fixture_id integer)
RETURNS text
LANGUAGE sql
STABLE
AS $$
    SELECT 'fbf1:' ||
           f.sport || ':' ||
           f.season::text || ':' ||
           COALESCE(f.league_id, 0)::text || ':' ||
           f.home_team_id::text || ':' ||
           f.away_team_id::text || ':' ||
           COALESCE(f.home_score::text, '') || ':' ||
           COALESCE(f.away_score::text, '') || ':' ||
           COALESCE(
               (
                   SELECT string_agg(pfm.provider || '=' || pfm.provider_fixture_id, ',' ORDER BY pfm.provider)
                   FROM public.provider_fixture_map pfm
                   WHERE pfm.fixture_id = f.id
                     AND pfm.sport = f.sport
               ),
               ''
           )
    FROM public.fixtures f
    WHERE f.id = p_fixture_id;
$$;

CREATE OR REPLACE FUNCTION public.enqueue_fixture_boxscore(p_fixture_id integer)
RETURNS boolean
LANGUAGE plpgsql
AS $$
DECLARE
    v_sport text;
    v_status text;
    v_input_version text;
BEGIN
    SELECT sport, status, public.fixture_boxscore_input_version(id)
      INTO v_sport, v_status, v_input_version
      FROM public.fixtures
     WHERE id = p_fixture_id;

    IF v_sport IS NULL THEN
        RETURN false;
    END IF;
    IF v_status NOT IN ('completed', 'seeded') THEN
        RETURN false;
    END IF;
    IF v_input_version IS NULL THEN
        RETURN false;
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('fixture_boxscore', 'fixture', p_fixture_id, v_sport, 'pending',
            v_input_version, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN true;
END;
$$;

CREATE OR REPLACE FUNCTION public.enqueue_recent_fixture_boxscores(
    p_sport text DEFAULT NULL,
    p_since interval DEFAULT INTERVAL '14 days',
    p_limit integer DEFAULT 100
)
RETURNS integer
LANGUAGE plpgsql
AS $$
DECLARE
    r record;
    v_enqueued integer := 0;
BEGIN
    FOR r IN
        SELECT f.id
          FROM public.fixtures f
          LEFT JOIN public.fixture_boxscore_fetches b ON b.fixture_id = f.id
         WHERE f.status IN ('completed', 'seeded')
           AND f.start_time >= NOW() - p_since
           AND (p_sport IS NULL OR f.sport = p_sport)
           AND (
                b.fixture_id IS NULL
             OR b.status IN ('fetch_failed', 'parse_failed', 'validation_failed', 'blocked')
           )
         ORDER BY f.start_time DESC
         LIMIT GREATEST(COALESCE(p_limit, 100), 0)
    LOOP
        IF public.enqueue_fixture_boxscore(r.id) THEN
            v_enqueued := v_enqueued + 1;
        END IF;
    END LOOP;
    RETURN v_enqueued;
END;
$$;

CREATE OR REPLACE FUNCTION public.enqueue_fixture_boxscore_on_final()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.status IN ('completed', 'seeded') THEN
        IF TG_OP = 'INSERT' THEN
            PERFORM public.enqueue_fixture_boxscore(NEW.id);
        ELSIF OLD.status IS DISTINCT FROM NEW.status
           OR OLD.home_score IS DISTINCT FROM NEW.home_score
           OR OLD.away_score IS DISTINCT FROM NEW.away_score
           OR OLD.external_id IS DISTINCT FROM NEW.external_id
           OR OLD.home_team_id IS DISTINCT FROM NEW.home_team_id
           OR OLD.away_team_id IS DISTINCT FROM NEW.away_team_id
        THEN
            PERFORM public.enqueue_fixture_boxscore(NEW.id);
        END IF;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS fixture_boxscore_enqueue_on_final ON public.fixtures;
CREATE TRIGGER fixture_boxscore_enqueue_on_final
    AFTER INSERT OR UPDATE OF status, home_score, away_score, external_id, home_team_id, away_team_id
    ON public.fixtures
    FOR EACH ROW
    EXECUTE FUNCTION public.enqueue_fixture_boxscore_on_final();

CREATE OR REPLACE FUNCTION public.enqueue_fixture_boxscore_on_provider_map()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    PERFORM public.enqueue_fixture_boxscore(NEW.fixture_id);
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS fixture_boxscore_enqueue_on_provider_map ON public.provider_fixture_map;
CREATE TRIGGER fixture_boxscore_enqueue_on_provider_map
    AFTER INSERT OR UPDATE OF provider_fixture_id
    ON public.provider_fixture_map
    FOR EACH ROW
    EXECUTE FUNCTION public.enqueue_fixture_boxscore_on_provider_map();

INSERT INTO public.schema_migrations(version) VALUES ('189_fixture_boxscore_fetches')
    ON CONFLICT DO NOTHING;

COMMIT;

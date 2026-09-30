BEGIN;

CREATE TABLE IF NOT EXISTS public.harvester_insider_wraps (
    team_id integer NOT NULL,
    sport text NOT NULL REFERENCES public.sports(id),
    work_version text NOT NULL,
    entity_type text NOT NULL CHECK (entity_type IN ('team','player')),
    entity_id integer NOT NULL,
    status text NOT NULL DEFAULT 'pending' CHECK (status IN ('pending','scored','skipped')),
    score_id bigint REFERENCES public.insider_scores(id),
    product_ref jsonb,
    updated_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (team_id,sport,work_version,entity_type,entity_id)
);

CREATE INDEX IF NOT EXISTS harvester_insider_wraps_pending_idx
    ON public.harvester_insider_wraps(team_id,sport,work_version,entity_type,entity_id)
    WHERE status='pending';

COMMENT ON TABLE public.harvester_insider_wraps IS
    'One claim-fenced Insider scored-board obligation per team/player after Harvester source pair verdicts. Work revision keeps a new source batch distinct, and terminal skipped/scored rows make every wrap auditable.';

INSERT INTO public.schema_migrations(version)
VALUES ('280_harvester_insider_wraps') ON CONFLICT (version) DO NOTHING;

COMMIT;

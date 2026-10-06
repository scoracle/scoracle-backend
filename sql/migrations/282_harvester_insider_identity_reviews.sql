BEGIN;

CREATE TABLE IF NOT EXISTS public.harvester_insider_identity_reviews (
    transfer_rumor_id bigint PRIMARY KEY REFERENCES public.transfer_rumors(id) ON DELETE CASCADE,
    classification_id bigint NOT NULL REFERENCES public.harvester_classifications(id) ON DELETE CASCADE,
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    team_id integer NOT NULL,
    player_id integer NOT NULL,
    sport text NOT NULL REFERENCES public.sports(id),
    status text NOT NULL DEFAULT 'pending' CHECK (status IN
        ('pending','skipped','applied','rejected','failed_closed')),
    reason text,
    application_id bigint REFERENCES public.transfer_identity_applications(id),
    model_version text,
    prompt_version text,
    model_raw text,
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS harvester_insider_identity_pending_idx
    ON public.harvester_insider_identity_reviews(team_id,sport,transfer_rumor_id)
    WHERE status='pending';

COMMENT ON TABLE public.harvester_insider_identity_reviews IS
    'Claim-fenced source-only identity adjudication obligation for each Harvester player transfer verdict. The settled-sources route still depends on Editor links and is not used here; ineligible pairs close with an explicit reason.';

INSERT INTO public.schema_migrations(version)
VALUES ('282_harvester_insider_identity_reviews') ON CONFLICT (version) DO NOTHING;

COMMIT;

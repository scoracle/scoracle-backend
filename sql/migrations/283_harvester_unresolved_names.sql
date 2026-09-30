BEGIN;

CREATE TABLE IF NOT EXISTS public.harvester_unresolved_names (
    article_id bigint NOT NULL REFERENCES public.news_articles(id) ON DELETE CASCADE,
    sport text NOT NULL REFERENCES public.sports(id),
    matched_norm text NOT NULL,
    reason text NOT NULL CHECK (reason IN ('ambiguous_surface','alias_only')),
    candidates jsonb NOT NULL,
    body_sha256 text NOT NULL,
    opening_sha256 text NOT NULL,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (article_id,sport,matched_norm)
);

CREATE INDEX IF NOT EXISTS harvester_unresolved_names_sport_idx
    ON public.harvester_unresolved_names(sport,created_at DESC,reason);

COMMENT ON TABLE public.harvester_unresolved_names IS
    'Source-bound, explicit unresolved name surfaces. Shared names and alias-only matches do not become authoritative links or character identities; candidate IDs are review evidence only.';

INSERT INTO public.schema_migrations(version)
VALUES ('283_harvester_unresolved_names') ON CONFLICT (version) DO NOTHING;

COMMIT;

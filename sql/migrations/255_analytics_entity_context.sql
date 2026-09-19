-- DuckDB analytics POC (Phase 3): derived cohort-context snapshot.
-- Postgres remains the system of record; this table holds ONLY derived,
-- season-grain analytical snapshots produced by the DuckDB engine (the
-- analytics boundary in go/internal/analytics) and written back by a Go
-- batch job. DuckDB itself never writes here — the job owns the write.
-- Rows are pure derived knowledge recomputable from player_stats/team_stats;
-- nothing operational reads them yet (memories.rs consumes them read-only).
BEGIN;

CREATE TABLE IF NOT EXISTS public.analytics_entity_context (
    sport             text        NOT NULL REFERENCES sports(id),
    entity_type       text        NOT NULL CHECK (entity_type IN ('player','team')),
    entity_id         integer     NOT NULL,
    season            integer     NOT NULL,
    league_id         integer     NOT NULL,
    rating            numeric     NOT NULL,
    prior_season      integer,
    prior_rating      numeric,
    delta             numeric,
    delta_pctile      numeric,
    peer_count        integer     NOT NULL,
    peer_delta_median numeric,
    peer_delta_p25    numeric,
    peer_delta_p75    numeric,
    computed_at       timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (sport, entity_type, entity_id, season, league_id)
);

COMMENT ON TABLE public.analytics_entity_context IS
    'DuckDB-derived season-grain context: an entity''s rating arc and its '
    'season-over-season delta against the league-cohort delta distribution '
    '(median/p25/p75). Recomputable from player_stats/team_stats ratings; '
    'never authoritative.';

CREATE INDEX IF NOT EXISTS analytics_entity_context_entity_idx
    ON public.analytics_entity_context (sport, entity_type, entity_id, season);

INSERT INTO public.schema_migrations(version) VALUES ('255_analytics_entity_context') ON CONFLICT DO NOTHING;
COMMIT;

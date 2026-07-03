-- 117_current_identity_roster_first.sql
--
-- Stop treating "last seeded" player metadata as current identity. Retro-seeding
-- historical seasons can carry an old team in provider player payloads; if that
-- value wins, downstream transfer/trade prompts can misclassify old moves as live
-- rumors. Current identity must be roster/override-first, with historical stats
-- as fallback and players.team_id as legacy last resort only.

-- Explicit current-team override hook for applied transfers/trades and manual
-- corrections. This is deliberately separate from historical stats.
CREATE TABLE IF NOT EXISTS public.player_current_identity_overrides (
    id                  BIGSERIAL PRIMARY KEY,
    sport               TEXT NOT NULL REFERENCES public.sports(id),
    player_id           INTEGER NOT NULL,
    team_id             INTEGER,
    league_id           INTEGER,
    source              TEXT NOT NULL DEFAULT 'manual',
    source_rumor_id     BIGINT,
    source_synthesis_id BIGINT,
    confidence          NUMERIC(4,3),
    reason              TEXT,
    evidence            JSONB NOT NULL DEFAULT '{}'::jsonb,
    applied_at          TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    applied_by          TEXT,
    reverted_at         TIMESTAMPTZ,
    reverted_by         TEXT,
    revert_reason       TEXT,
    FOREIGN KEY (player_id, sport) REFERENCES public.players(id, sport) ON DELETE CASCADE,
    FOREIGN KEY (team_id, sport) REFERENCES public.teams(id, sport) ON DELETE RESTRICT
);

CREATE INDEX IF NOT EXISTS idx_player_current_identity_overrides_active
    ON public.player_current_identity_overrides (sport, player_id, applied_at DESC, id DESC)
    WHERE reverted_at IS NULL;

CREATE INDEX IF NOT EXISTS idx_player_current_identity_overrides_source_rumor
    ON public.player_current_identity_overrides (source_rumor_id)
    WHERE source_rumor_id IS NOT NULL;

-- Full canonical current identity. Source priority:
--   override -> active roster -> latest stats -> legacy players row.
CREATE OR REPLACE VIEW public.player_current_identity AS
WITH active_override AS (
    SELECT DISTINCT ON (o.sport, o.player_id)
        o.sport,
        o.player_id,
        o.team_id,
        o.league_id,
        NULL::text AS position,
        NULL::text AS position_group,
        NULL::text AS jersey_number,
        'override'::text AS source,
        o.applied_at AS source_updated_at
    FROM public.player_current_identity_overrides o
    WHERE o.reverted_at IS NULL
    ORDER BY o.sport, o.player_id, o.applied_at DESC, o.id DESC
),
roster_current AS (
    SELECT DISTINCT ON (tr.sport, tr.player_id)
        tr.sport,
        tr.player_id,
        tr.team_id,
        t.league_id,
        tr.position,
        tr.position_group,
        tr.jersey_number,
        'roster'::text AS source,
        tr.last_seen AS source_updated_at
    FROM public.team_rosters tr
    LEFT JOIN public.teams t ON t.id = tr.team_id AND t.sport = tr.sport
    WHERE tr.is_active
    ORDER BY tr.sport, tr.player_id, tr.season DESC, tr.last_seen DESC NULLS LAST
),
stats_current AS (
    SELECT DISTINCT ON (ps.sport, ps.player_id)
        ps.sport,
        ps.player_id,
        ps.team_id,
        NULLIF(ps.league_id, 0) AS league_id,
        ps.position,
        public.position_group(ps.sport, ps.position) AS position_group,
        NULL::text AS jersey_number,
        'stats'::text AS source,
        ps.updated_at AS source_updated_at
    FROM public.player_stats ps
    WHERE ps.team_id IS NOT NULL
    ORDER BY ps.sport, ps.player_id, ps.season DESC NULLS LAST, ps.updated_at DESC NULLS LAST
)
SELECT
    p.sport,
    p.id AS player_id,
    COALESCE(ao.team_id, rc.team_id, sc.team_id, p.team_id) AS team_id,
    COALESCE(ao.league_id, rc.league_id, sc.league_id, p.league_id) AS league_id,
    COALESCE(ao.position, rc.position, sc.position) AS position,
    COALESCE(ao.position_group, rc.position_group, sc.position_group) AS position_group,
    COALESCE(ao.jersey_number, rc.jersey_number) AS jersey_number,
    CASE
        WHEN ao.player_id IS NOT NULL THEN ao.source
        WHEN rc.player_id IS NOT NULL THEN rc.source
        WHEN sc.player_id IS NOT NULL THEN sc.source
        WHEN p.team_id IS NOT NULL OR p.league_id IS NOT NULL THEN 'legacy_player'
        ELSE NULL
    END AS source,
    COALESCE(ao.source_updated_at, rc.source_updated_at, sc.source_updated_at, p.updated_at) AS source_updated_at
FROM public.players p
LEFT JOIN active_override ao ON ao.sport = p.sport AND ao.player_id = p.id
LEFT JOIN roster_current rc ON rc.sport = p.sport AND rc.player_id = p.id
LEFT JOIN stats_current sc ON sc.sport = p.sport AND sc.player_id = p.id;

COMMENT ON VIEW public.player_current_identity IS
  'Canonical current player identity: applied override, then active roster, then latest stats, then legacy players row. Use for meta, autofill, and transfer prompts.';

-- Compatibility view for existing autofill materialized views and readers.
CREATE OR REPLACE VIEW public.player_current_team AS
SELECT player_id, sport, team_id
FROM public.player_current_identity
WHERE team_id IS NOT NULL;

COMMENT ON VIEW public.player_current_team IS
  'Compatibility current-team view backed by public.player_current_identity; never read raw players.team_id for current team unless this view falls back to legacy_player.';

-- Keep the older roster-first view name aligned for any in-flight consumers.
CREATE OR REPLACE VIEW public.player_current_team_roster AS
SELECT
    sport,
    player_id,
    team_id,
    position,
    position_group,
    jersey_number,
    source
FROM public.player_current_identity;

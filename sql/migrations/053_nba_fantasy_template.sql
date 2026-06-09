-- ============================================================================
-- 053_nba_fantasy_template.sql
-- NBA Fantasy-mode pizza: render the DraftKings scoring components as the pizza when the
-- Composite card's Regular|Fantasy selector is on Fantasy. Regular keeps the z-datapoint
-- pizza (unchanged). Mirrors the NFL offensive-skill templates (047) but for NBA, position-
-- agnostic ('ALL'). Re-adds the NBA branch to position_group (removed when Phase 3 was
-- scoped to NFL) + seeds the NBA stat_templates rows.
--
-- Pure metadata — no recompute. The wedge values + within-position percentiles (incl. the
-- per_36/per_season rate siblings) already exist in player_stats.stats/percentiles.
-- Frontend gates the template to Fantasy mode (Regular always shows z-stats).
--
-- Apply with: sql/migrate.sh   (or psql -f ...)
-- ============================================================================

BEGIN;

CREATE OR REPLACE FUNCTION public.position_group(p_sport TEXT, p_position TEXT)
RETURNS TEXT LANGUAGE sql IMMUTABLE AS $$
    SELECT CASE upper(p_sport)
        WHEN 'NBA' THEN 'ALL'  -- one Fantasy-mode template for all NBA players
        WHEN 'NFL' THEN CASE p_position
            WHEN 'Quarterback' THEN 'quarterback'
            WHEN 'QB'          THEN 'quarterback'
            WHEN 'Running Back' THEN 'running-back'
            WHEN 'Fullback'     THEN 'running-back'
            WHEN 'RB'           THEN 'running-back'
            WHEN 'FB'           THEN 'running-back'
            WHEN 'HB'           THEN 'running-back'
            WHEN 'Wide Receiver' THEN 'receiver'
            WHEN 'Tight End'     THEN 'receiver'
            WHEN 'WR'            THEN 'receiver'
            WHEN 'TE'            THEN 'receiver'
            ELSE NULL
        END
        ELSE NULL
    END;
$$;

-- NBA Fantasy-mode template (DraftKings scoring components). 'turnover' siblings use the
-- legacy 'tov_' alias (rate_base). The stat_templates table is defined in shared.sql.
INSERT INTO public.stat_templates (sport, position_group, stat_key, rate_base, sort_order) VALUES
    ('NBA', 'ALL', 'pts',      NULL,  10),
    ('NBA', 'ALL', 'reb',      NULL,  20),
    ('NBA', 'ALL', 'ast',      NULL,  30),
    ('NBA', 'ALL', 'stl',      NULL,  40),
    ('NBA', 'ALL', 'blk',      NULL,  50),
    ('NBA', 'ALL', 'fg3m',     NULL,  60),
    ('NBA', 'ALL', 'turnover', 'tov', 70)
ON CONFLICT (sport, position_group, stat_key) DO NOTHING;

INSERT INTO public.schema_migrations (version) VALUES ('053_nba_fantasy_template')
ON CONFLICT (version) DO NOTHING;

COMMIT;

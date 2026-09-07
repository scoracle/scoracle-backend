-- 248: the pizza learns the FPL era — templates by variant, Forwards found.
--
-- Scott, 2026-09-07: "Now we just need the frontend to match." The Profile
-- card's Regular-mode pizza draws from stat_templates, which carried only the
-- vendor-era vocabulary (shots_on_target, key_passes, duels_won…) — an FPL-era
-- season row would render a pizza of zero wedges. And position_group knew the
-- vendor's "Attacker" but not FPL's "Forward", so every forward lost their
-- template entirely and fell back to the z-score pizza.
--
-- Templates now carry a variant ('vendor' | 'fpl') and the template functions
-- pick by ROW SHAPE: a season row bearing any FPL-only bookkeeping key
-- (expected_goals_conceded / defensive_contribution / ict_index / bps for
-- players, expected_goals_for for teams) gets the fpl wedges; anything else
-- gets the vendor wedges. Seasons are era-homogeneous, so each season's pizza
-- is native to its own data. Facet names reuse the vendor set exactly
-- (attacking/passing/defending/shot-stopping/offense/defense) — the frontend
-- groups by facet string and needs no change.
BEGIN;

ALTER TABLE stat_templates ADD COLUMN IF NOT EXISTS variant text NOT NULL DEFAULT 'vendor';
ALTER TABLE stat_templates DROP CONSTRAINT stat_templates_pkey;
ALTER TABLE stat_templates ADD PRIMARY KEY (sport, position_group, stat_key, variant);

CREATE OR REPLACE FUNCTION public.position_group(p_sport text, p_position text)
 RETURNS text
 LANGUAGE sql
 IMMUTABLE
AS $function$
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
        WHEN 'FOOTBALL' THEN CASE p_position
            WHEN 'Goalkeeper' THEN 'goalkeeper'
            WHEN 'Defender'   THEN 'defender'
            WHEN 'Midfielder' THEN 'midfielder'
            WHEN 'Attacker'   THEN 'attacker'
            WHEN 'Forward'    THEN 'attacker'  -- FPL's word for the same group
            ELSE NULL
        END
        ELSE NULL
    END;
$function$;

INSERT INTO stat_templates (sport, position_group, stat_key, facet, sort_order, variant) VALUES
    -- goalkeeper
    ('FOOTBALL', 'goalkeeper', 'saves',                      'shot-stopping', 10, 'fpl'),
    ('FOOTBALL', 'goalkeeper', 'save_pct',                   'shot-stopping', 11, 'fpl'),
    ('FOOTBALL', 'goalkeeper', 'goals_conceded',             'shot-stopping', 12, 'fpl'),
    ('FOOTBALL', 'goalkeeper', 'expected_goals_conceded',    'shot-stopping', 13, 'fpl'),
    ('FOOTBALL', 'goalkeeper', 'clean_sheets',               'shot-stopping', 14, 'fpl'),
    ('FOOTBALL', 'goalkeeper', 'penalties_saved',            'shot-stopping', 15, 'fpl'),
    -- defender
    ('FOOTBALL', 'defender',   'tackles',                    'defending', 10, 'fpl'),
    ('FOOTBALL', 'defender',   'cbi',                        'defending', 11, 'fpl'),
    ('FOOTBALL', 'defender',   'defensive_contribution',     'defending', 12, 'fpl'),
    ('FOOTBALL', 'defender',   'ball_recovery',              'defending', 13, 'fpl'),
    ('FOOTBALL', 'defender',   'clean_sheets',               'defending', 14, 'fpl'),
    ('FOOTBALL', 'defender',   'expected_assists',           'passing',   20, 'fpl'),
    ('FOOTBALL', 'defender',   'goals',                      'attacking', 30, 'fpl'),
    ('FOOTBALL', 'defender',   'assists',                    'attacking', 31, 'fpl'),
    ('FOOTBALL', 'defender',   'expected_goal_involvements', 'attacking', 32, 'fpl'),
    -- midfielder
    ('FOOTBALL', 'midfielder', 'expected_assists',           'passing',   10, 'fpl'),
    ('FOOTBALL', 'midfielder', 'goals',                      'attacking', 20, 'fpl'),
    ('FOOTBALL', 'midfielder', 'expected_goals',             'attacking', 21, 'fpl'),
    ('FOOTBALL', 'midfielder', 'assists',                    'attacking', 22, 'fpl'),
    ('FOOTBALL', 'midfielder', 'expected_goal_involvements', 'attacking', 23, 'fpl'),
    ('FOOTBALL', 'midfielder', 'tackles',                    'defending', 30, 'fpl'),
    ('FOOTBALL', 'midfielder', 'defensive_contribution',     'defending', 31, 'fpl'),
    ('FOOTBALL', 'midfielder', 'ball_recovery',              'defending', 32, 'fpl'),
    ('FOOTBALL', 'midfielder', 'cbi',                        'defending', 33, 'fpl'),
    -- attacker (FPL "Forward")
    ('FOOTBALL', 'attacker',   'goals',                      'attacking', 10, 'fpl'),
    ('FOOTBALL', 'attacker',   'expected_goals',             'attacking', 11, 'fpl'),
    ('FOOTBALL', 'attacker',   'assists',                    'attacking', 12, 'fpl'),
    ('FOOTBALL', 'attacker',   'expected_goal_involvements', 'attacking', 13, 'fpl'),
    ('FOOTBALL', 'attacker',   'expected_assists',           'passing',   20, 'fpl'),
    ('FOOTBALL', 'attacker',   'tackles',                    'defending', 30, 'fpl'),
    ('FOOTBALL', 'attacker',   'defensive_contribution',     'defending', 31, 'fpl'),
    ('FOOTBALL', 'attacker',   'ball_recovery',              'defending', 32, 'fpl'),
    -- team
    ('FOOTBALL', 'team',       'goals_for',                  'offense', 10, 'fpl'),
    ('FOOTBALL', 'team',       'expected_goals_for',         'offense', 11, 'fpl'),
    ('FOOTBALL', 'team',       'assists',                    'offense', 12, 'fpl'),
    ('FOOTBALL', 'team',       'goals_against',              'defense', 20, 'fpl'),
    ('FOOTBALL', 'team',       'expected_goals_against',     'defense', 21, 'fpl'),
    ('FOOTBALL', 'team',       'clean_sheets',               'defense', 22, 'fpl'),
    ('FOOTBALL', 'team',       'tackles',                    'defense', 23, 'fpl'),
    ('FOOTBALL', 'team',       'saves',                      'defense', 24, 'fpl'),
    ('FOOTBALL', 'team',       'ball_recovery',              'defense', 25, 'fpl')
ON CONFLICT DO NOTHING;

CREATE OR REPLACE FUNCTION public.template_block(p_sport text, p_position text, p_stats jsonb, p_pct jsonb, p_scoped jsonb)
 RETURNS jsonb
 LANGUAGE sql
 STABLE
AS $function$
    WITH tmpl AS (
        SELECT t.stat_key, COALESCE(sd.rate_base, t.stat_key) AS rate_base,
               COALESCE(sd.display_name, t.stat_key) AS label, t.facet, t.sort_order
        FROM public.stat_templates t
        LEFT JOIN public.stat_definitions sd
               ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'player'
        WHERE t.sport = upper(p_sport) AND t.position_group = public.position_group(p_sport, p_position)
          -- Era by row shape: seasons are homogeneous, so a row bearing any
          -- FPL-only bookkeeping key gets the fpl wedges, else the vendor set.
          AND t.variant = CASE WHEN upper(p_sport) = 'FOOTBALL'
                                AND (p_stats ? 'expected_goals_conceded' OR p_stats ? 'defensive_contribution'
                                     OR p_stats ? 'ict_index' OR p_stats ? 'bps')
                               THEN 'fpl' ELSE 'vendor' END
    ),
    modes(mode, suffix) AS (
        SELECT 'default'::text, ''::text
        UNION SELECT DISTINCT rm.mode, rm.suffix FROM public.rate_modes rm
    )
    SELECT jsonb_object_agg(m.mode, m.items)
    FROM (
        SELECT md.mode,
            (SELECT jsonb_agg(jsonb_build_object(
                'key',   t.stat_key,
                'label', t.label,
                'value', COALESCE((p_stats->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_stats->>t.stat_key)::numeric, 0),
                'pct',   COALESCE((p_pct->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric,
                                  (p_pct->>t.stat_key)::numeric, 0),
                'scoped_pct', (
                    SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>(CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))::numeric)
                                  FILTER (WHERE s.keys ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END)), '{}'::jsonb)
                    FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
                ),
                'facet', t.facet,
                'sort',  t.sort_order
            ) ORDER BY t.sort_order) FROM tmpl t) AS items
        FROM modes md
        WHERE EXISTS (SELECT 1 FROM tmpl t
                      WHERE p_stats ? (CASE WHEN md.suffix='' THEN t.stat_key ELSE t.rate_base||md.suffix END))
    ) m
    WHERE m.items IS NOT NULL;
$function$;

CREATE OR REPLACE FUNCTION public.team_template_block(p_sport text, p_stats jsonb, p_pct jsonb, p_scoped jsonb)
 RETURNS jsonb
 LANGUAGE sql
 STABLE
AS $function$
    SELECT jsonb_build_object('default', jsonb_agg(jsonb_build_object(
               'key', t.stat_key, 'label', COALESCE(sd.display_name, t.stat_key),
               'value', COALESCE((p_stats->>t.stat_key)::numeric, 0),
               'pct',   COALESCE((p_pct->>t.stat_key)::numeric, 0),
               'scoped_pct', (
                   SELECT NULLIF(jsonb_object_agg(s.scope, (s.keys->>t.stat_key)::numeric)
                                 FILTER (WHERE s.keys ? t.stat_key), '{}'::jsonb)
                   FROM jsonb_each(COALESCE(p_scoped,'{}'::jsonb)) s(scope, keys)
               ),
               'facet', t.facet, 'sort', t.sort_order
           ) ORDER BY t.sort_order, t.stat_key))
    FROM public.stat_templates t
    LEFT JOIN public.stat_definitions sd
           ON sd.sport = t.sport AND sd.key_name = t.stat_key AND sd.entity_type = 'team'
    WHERE t.sport = upper(p_sport) AND t.position_group = 'team'
      AND t.variant = CASE WHEN upper(p_sport) = 'FOOTBALL' AND p_stats ? 'expected_goals_for'
                           THEN 'fpl' ELSE 'vendor' END
    HAVING count(*) > 0;
$function$;

-- self-recording (the file manages its own transaction; the runner's INSERT
-- is the idempotent backstop)
INSERT INTO schema_migrations (version) VALUES ('248_fpl_era_pizza_templates') ON CONFLICT DO NOTHING;

COMMIT;

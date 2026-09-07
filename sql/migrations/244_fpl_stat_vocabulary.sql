-- 244: the FPL-era stat vocabulary — the full fantasy payload, curated.
--
-- Scott, 2026-09-07: "I want to make sure that we're using the full EPL
-- fantasy data payload. I want to curate the data that we serve up."
--
-- The percentile/rating machinery is key-agnostic (recalculate_percentiles
-- sweeps whatever keys the season's rows carry), so the z-model adapts to the
-- new format through these definitions: is_percentile_eligible decides what
-- the model grades, is_inverse the direction, category the pizza facet.
-- Eligibility below is the PROPOSED curation — flipping a flag is data, not a
-- deploy, so pruning the served set is an UPDATE away.
--
-- Served (eligible): the fan-legible production and defensive work rates.
-- Internal (ineligible): fantasy bookkeeping (bps, ICT components) — stored
-- for future surfaces, never graded.
BEGIN;

INSERT INTO stat_definitions
    (sport, key_name, display_name, entity_type, category, is_inverse, is_derived,
     is_percentile_eligible, sort_order, unit, comparable)
VALUES
    -- Player: production
    ('FOOTBALL', 'clean_sheets',               'Clean Sheets',                 'player', 'goalkeeper', false, false, true,  40, 'cumulative_total', false),
    ('FOOTBALL', 'expected_assists',           'Expected Assists (xA)',        'player', 'passing',    false, false, true,  41, 'cumulative_total', false),
    ('FOOTBALL', 'expected_goal_involvements', 'Expected Involvements (xGI)',  'player', 'scoring',    false, false, true,  42, 'cumulative_total', false),
    ('FOOTBALL', 'expected_goals_conceded',    'Expected Goals Conceded (xGC)','player', 'goalkeeper', true,  false, true,  43, 'cumulative_total', false),
    ('FOOTBALL', 'defensive_contribution',     'Defensive Contribution',       'player', 'defensive',  false, false, true,  44, 'cumulative_total', false),
    ('FOOTBALL', 'cbi',                        'Clearances/Blocks/Intercepts', 'player', 'defensive',  false, false, true,  45, 'cumulative_total', false),
    ('FOOTBALL', 'bonus_points',               'Bonus Points',                 'player', 'fantasy',    false, false, true,  46, 'cumulative_total', false),
    ('FOOTBALL', 'ict_index',                  'ICT Index',                    'player', 'fantasy',    false, false, true,  47, 'cumulative_total', false),
    -- Player: internal (stored, never graded — ICT components + raw bps)
    ('FOOTBALL', 'bps',                        'Bonus Point System',           'player', 'fantasy',    false, false, false, 48, 'cumulative_total', false),
    ('FOOTBALL', 'influence',                  'Influence',                    'player', 'fantasy',    false, false, false, 49, 'cumulative_total', false),
    ('FOOTBALL', 'creativity',                 'Creativity',                   'player', 'fantasy',    false, false, false, 50, 'cumulative_total', false),
    ('FOOTBALL', 'threat',                     'Threat',                       'player', 'fantasy',    false, false, false, 51, 'cumulative_total', false),
    -- Team: derived from the fixture + player sums
    ('FOOTBALL', 'clean_sheets',               'Clean Sheets',                 'team',   'defensive',  false, false, true,  40, 'cumulative_total', false),
    ('FOOTBALL', 'expected_goals_for',         'Expected Goals For (xG)',      'team',   'attacking',  false, false, true,  41, 'cumulative_total', false),
    ('FOOTBALL', 'expected_goals_against',     'Expected Goals Against (xGA)', 'team',   'defensive',  true,  false, true,  42, 'cumulative_total', false)
ON CONFLICT DO NOTHING;

COMMIT;

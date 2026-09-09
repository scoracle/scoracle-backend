-- Team identity colors live with canonical metadata, not in a client palette.
BEGIN;
SET LOCAL lock_timeout = '5s';

ALTER TABLE public.teams
    ADD COLUMN IF NOT EXISTS primary_color text,
    ADD COLUMN IF NOT EXISTS secondary_color text,
    ADD COLUMN IF NOT EXISTS color_source text;

ALTER TABLE public.teams
    ADD CONSTRAINT teams_primary_color_hex CHECK (primary_color ~ '^#[0-9A-Fa-f]{6}$'),
    ADD CONSTRAINT teams_secondary_color_hex CHECK (secondary_color ~ '^#[0-9A-Fa-f]{6}$'),
    ADD CONSTRAINT teams_color_pair CHECK (num_nonnulls(primary_color, secondary_color) IN (0, 2)),
    ADD CONSTRAINT teams_color_source_required CHECK (primary_color IS NULL OR NULLIF(btrim(color_source), '') IS NOT NULL);

COMMENT ON COLUMN public.teams.primary_color IS
    'Canonical primary team color as #RRGGBB. Clients may soften it for presentation; never store a page-specific tint here.';
COMMENT ON COLUMN public.teams.secondary_color IS
    'Canonical secondary team color as #RRGGBB. Players inherit the colors of their current team through player_current_identity.';
COMMENT ON COLUMN public.teams.color_source IS
    'Provenance for the stored color pair. Unknown colors remain NULL; identity enrichment does not invent palettes.';

-- Reviewed ESPN published team palettes, retrieved 2026-09-09. Football is
-- intentionally unseeded: several alternate colors describe kits/placeholders.
CREATE TEMP TABLE team_color_seed (
    sport text NOT NULL, id integer NOT NULL, name text NOT NULL,
    primary_color text NOT NULL, secondary_color text NOT NULL, color_source text NOT NULL,
    PRIMARY KEY (sport, id)
) ON COMMIT DROP;
INSERT INTO team_color_seed VALUES
    ('NBA', 1, 'Atlanta Hawks', '#C8102E', '#FDB927', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/1'),
    ('NBA', 2, 'Boston Celtics', '#008348', '#FFFFFF', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/2'),
    ('NBA', 3, 'Brooklyn Nets', '#000000', '#FFFFFF', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/17'),
    ('NBA', 4, 'Charlotte Hornets', '#008CA8', '#1D1060', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/30'),
    ('NBA', 5, 'Chicago Bulls', '#CE1141', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/4'),
    ('NBA', 6, 'Cleveland Cavaliers', '#860038', '#BC945C', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/5'),
    ('NBA', 7, 'Dallas Mavericks', '#0064B1', '#BBC4CA', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/6'),
    ('NBA', 8, 'Denver Nuggets', '#0E2240', '#FEC524', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/7'),
    ('NBA', 9, 'Detroit Pistons', '#1D428A', '#C8102E', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/8'),
    ('NBA', 10, 'Golden State Warriors', '#FDB927', '#1D428A', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/9'),
    ('NBA', 11, 'Houston Rockets', '#CE0E2D', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/10'),
    ('NBA', 12, 'Indiana Pacers', '#0C2340', '#FFD520', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/11'),
    ('NBA', 13, 'LA Clippers', '#12173F', '#C8102E', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/12'),
    ('NBA', 14, 'Los Angeles Lakers', '#552583', '#FDB927', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/13'),
    ('NBA', 15, 'Memphis Grizzlies', '#5D76A9', '#12173F', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/29'),
    ('NBA', 16, 'Miami Heat', '#98002E', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/14'),
    ('NBA', 17, 'Milwaukee Bucks', '#00471B', '#EEE1C6', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/15'),
    ('NBA', 18, 'Minnesota Timberwolves', '#266092', '#79BC43', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/16'),
    ('NBA', 19, 'New Orleans Pelicans', '#0A2240', '#B4975A', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/3'),
    ('NBA', 20, 'New York Knicks', '#1D428A', '#F58426', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/18'),
    ('NBA', 21, 'Oklahoma City Thunder', '#007AC1', '#EF3B24', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/25'),
    ('NBA', 22, 'Orlando Magic', '#0150B5', '#9CA0A3', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/19'),
    ('NBA', 23, 'Philadelphia 76ers', '#1D428A', '#E01234', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/20'),
    ('NBA', 24, 'Phoenix Suns', '#29127A', '#E56020', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/21'),
    ('NBA', 25, 'Portland Trail Blazers', '#E03A3E', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/22'),
    ('NBA', 26, 'Sacramento Kings', '#5A2D81', '#6A7A82', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/23'),
    ('NBA', 27, 'San Antonio Spurs', '#000000', '#C4CED4', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/24'),
    ('NBA', 28, 'Toronto Raptors', '#D91244', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/28'),
    ('NBA', 29, 'Utah Jazz', '#4E008E', '#79A3DC', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/26'),
    ('NBA', 30, 'Washington Wizards', '#E31837', '#002B5C', 'https://site.api.espn.com/apis/site/v2/sports/basketball/nba/teams/27'),
    ('NFL', 1, 'New England Patriots', '#002A5C', '#C60C30', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/17'),
    ('NFL', 3, 'Buffalo Bills', '#00338D', '#D50A0A', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/2'),
    ('NFL', 4, 'New York Jets', '#115740', '#FFFFFF', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/20'),
    ('NFL', 5, 'Miami Dolphins', '#008E97', '#FC4C02', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/15'),
    ('NFL', 6, 'Baltimore Ravens', '#29126F', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/33'),
    ('NFL', 7, 'Pittsburgh Steelers', '#000000', '#FFB612', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/23'),
    ('NFL', 8, 'Cleveland Browns', '#472A08', '#FF3C00', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/5'),
    ('NFL', 9, 'Cincinnati Bengals', '#FB4F14', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/4'),
    ('NFL', 10, 'Houston Texans', '#021018', '#EB0028', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/34'),
    ('NFL', 11, 'Tennessee Titans', '#4495D2', '#001532', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/10'),
    ('NFL', 12, 'Indianapolis Colts', '#003B75', '#FFFFFF', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/11'),
    ('NFL', 13, 'Jacksonville Jaguars', '#007487', '#D7A22A', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/30'),
    ('NFL', 14, 'Kansas City Chiefs', '#E31837', '#FFB612', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/12'),
    ('NFL', 15, 'Denver Broncos', '#0A2343', '#FC4C02', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/7'),
    ('NFL', 16, 'Las Vegas Raiders', '#000000', '#A5ACAF', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/13'),
    ('NFL', 17, 'Los Angeles Chargers', '#0080C6', '#FFC20E', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/24'),
    ('NFL', 18, 'Philadelphia Eagles', '#06424D', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/21'),
    ('NFL', 19, 'Dallas Cowboys', '#002A5C', '#B0B7BC', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/6'),
    ('NFL', 20, 'New York Giants', '#003C7F', '#C9243F', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/19'),
    ('NFL', 21, 'Washington Commanders', '#5A1414', '#FFB612', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/28'),
    ('NFL', 22, 'Green Bay Packers', '#204E32', '#FFB612', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/9'),
    ('NFL', 23, 'Minnesota Vikings', '#4F2683', '#FFC62F', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/16'),
    ('NFL', 24, 'Chicago Bears', '#0B1C3A', '#E64100', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/3'),
    ('NFL', 25, 'Detroit Lions', '#0076B6', '#BBBBBB', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/8'),
    ('NFL', 26, 'New Orleans Saints', '#D3BC8D', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/18'),
    ('NFL', 27, 'Atlanta Falcons', '#A71930', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/1'),
    ('NFL', 28, 'Tampa Bay Buccaneers', '#BD1C36', '#3E3A35', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/27'),
    ('NFL', 29, 'Carolina Panthers', '#0085CA', '#000000', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/29'),
    ('NFL', 30, 'San Francisco 49ers', '#AA0000', '#B3995D', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/25'),
    ('NFL', 31, 'Seattle Seahawks', '#002A5C', '#69BE28', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/26'),
    ('NFL', 32, 'Los Angeles Rams', '#003594', '#FFD100', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/14'),
    ('NFL', 33, 'Arizona Cardinals', '#A40227', '#FFFFFF', 'https://site.api.espn.com/apis/site/v2/sports/football/nfl/teams/22');

DO $$
BEGIN
    IF (SELECT count(*) FROM team_color_seed s JOIN public.teams t
        ON t.sport = s.sport AND t.id = s.id AND t.name = s.name) <> 62 THEN
        RAISE EXCEPTION 'Team color seed identity mismatch: expected 62 sport/id/name matches';
    END IF;
END;
$$;

UPDATE public.teams t
SET primary_color = s.primary_color, secondary_color = s.secondary_color,
    color_source = s.color_source
FROM team_color_seed s
WHERE t.sport = s.sport AND t.id = s.id AND t.name = s.name;


INSERT INTO public.schema_migrations(version) VALUES ('251_team_colors') ON CONFLICT DO NOTHING;
COMMIT;

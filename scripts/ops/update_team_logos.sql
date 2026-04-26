-- Update NBA + NFL team logos from Wikimedia public-domain sources.
-- Source: /home/sheneveld/Downloads/nba_nfl_logos.md (paths only; sizes
-- stripped per Wikimedia hotlink policy below).
--
-- IMPORTANT: We store the source SVG path, NOT the /thumb/.../NNNpx-X.svg.png
-- form. As of 2026-04-25, Wikimedia returns HTTP 400 on browser requests
-- to thumb URLs ("Use thumbnail steps listed on https://w.wiki/GHai").
-- Browsers render SVG natively at any CSS size — no thumbnail needed.
-- See scripts/ops/wikimedia_thumb_to_svg.sql for the migration script
-- that rewrote the previously-stored thumb URLs.
--
-- Run: psql "$DATABASE_PRIVATE_URL" -f scripts/ops/update_team_logos.sql

BEGIN;

-- NBA --------------------------------------------------------------------
UPDATE teams t
SET logo_url = v.url,
    updated_at = NOW()
FROM (VALUES
    ('Atlanta Hawks',          'https://upload.wikimedia.org/wikipedia/en/2/24/Atlanta_Hawks_logo.svg'),
    ('Boston Celtics',         'https://upload.wikimedia.org/wikipedia/en/8/8f/Boston_Celtics.svg'),
    ('Brooklyn Nets',          'https://upload.wikimedia.org/wikipedia/en/4/40/Brooklyn_Nets_primary_icon_logo_2024.svg'),
    ('Charlotte Hornets',      'https://upload.wikimedia.org/wikipedia/en/c/c4/Charlotte_Hornets_(2014).svg'),
    ('Chicago Bulls',          'https://upload.wikimedia.org/wikipedia/en/6/67/Chicago_Bulls_logo.svg'),
    ('Cleveland Cavaliers',    'https://upload.wikimedia.org/wikipedia/commons/4/4b/Cleveland_Cavaliers_logo.svg'),
    ('Dallas Mavericks',       'https://upload.wikimedia.org/wikipedia/en/9/97/Dallas_Mavericks_logo.svg'),
    ('Denver Nuggets',         'https://upload.wikimedia.org/wikipedia/en/7/76/Denver_Nuggets.svg'),
    ('Detroit Pistons',        'https://upload.wikimedia.org/wikipedia/commons/c/c9/Logo_of_the_Detroit_Pistons.svg'),
    ('Golden State Warriors',  'https://upload.wikimedia.org/wikipedia/en/0/01/Golden_State_Warriors_logo.svg'),
    ('Houston Rockets',        'https://upload.wikimedia.org/wikipedia/en/2/28/Houston_Rockets.svg'),
    ('Indiana Pacers',         'https://upload.wikimedia.org/wikipedia/en/1/1b/Indiana_Pacers.svg'),
    -- DB stores "LA Clippers"; markdown listed "Los Angeles Clippers".
    ('LA Clippers',            'https://upload.wikimedia.org/wikipedia/en/e/ed/Los_Angeles_Clippers_(2024).svg'),
    ('Los Angeles Lakers',     'https://upload.wikimedia.org/wikipedia/commons/3/3c/Los_Angeles_Lakers_logo.svg'),
    ('Memphis Grizzlies',      'https://upload.wikimedia.org/wikipedia/en/f/f1/Memphis_Grizzlies.svg'),
    ('Miami Heat',             'https://upload.wikimedia.org/wikipedia/en/f/fb/Miami_Heat_logo.svg'),
    ('Milwaukee Bucks',        'https://upload.wikimedia.org/wikipedia/en/4/4a/Milwaukee_Bucks_logo.svg'),
    ('Minnesota Timberwolves', 'https://upload.wikimedia.org/wikipedia/en/c/c2/Minnesota_Timberwolves_logo.svg'),
    ('New Orleans Pelicans',   'https://upload.wikimedia.org/wikipedia/en/0/0d/New_Orleans_Pelicans_logo.svg'),
    ('New York Knicks',        'https://upload.wikimedia.org/wikipedia/en/2/25/New_York_Knicks_logo.svg'),
    ('Oklahoma City Thunder',  'https://upload.wikimedia.org/wikipedia/en/5/5d/Oklahoma_City_Thunder.svg'),
    ('Orlando Magic',          'https://upload.wikimedia.org/wikipedia/en/1/10/Orlando_Magic_logo.svg'),
    ('Philadelphia 76ers',     'https://upload.wikimedia.org/wikipedia/en/0/0e/Philadelphia_76ers_logo.svg'),
    ('Phoenix Suns',           'https://upload.wikimedia.org/wikipedia/en/d/dc/Phoenix_Suns_logo.svg'),
    ('Portland Trail Blazers', 'https://upload.wikimedia.org/wikipedia/en/2/21/Portland_Trail_Blazers_logo.svg'),
    ('Sacramento Kings',       'https://upload.wikimedia.org/wikipedia/en/c/c7/SacramentoKings.svg'),
    ('San Antonio Spurs',      'https://upload.wikimedia.org/wikipedia/en/a/a2/San_Antonio_Spurs.svg'),
    ('Toronto Raptors',        'https://upload.wikimedia.org/wikipedia/en/3/36/Toronto_Raptors_logo.svg'),
    ('Utah Jazz',              'https://upload.wikimedia.org/wikipedia/en/7/77/Utah_Jazz_logo_2025.svg'),
    ('Washington Wizards',     'https://upload.wikimedia.org/wikipedia/en/0/02/Washington_Wizards_logo.svg')
) AS v(name, url)
WHERE t.sport = 'NBA' AND t.name = v.name;

-- NFL --------------------------------------------------------------------
UPDATE teams t
SET logo_url = v.url,
    updated_at = NOW()
FROM (VALUES
    ('Arizona Cardinals',     'https://upload.wikimedia.org/wikipedia/en/7/72/Arizona_Cardinals_logo.svg'),
    ('Atlanta Falcons',       'https://upload.wikimedia.org/wikipedia/en/c/c5/Atlanta_Falcons_logo.svg'),
    ('Baltimore Ravens',      'https://upload.wikimedia.org/wikipedia/en/1/16/Baltimore_Ravens_logo.svg'),
    ('Buffalo Bills',         'https://upload.wikimedia.org/wikipedia/en/7/77/Buffalo_Bills_logo.svg'),
    ('Carolina Panthers',     'https://upload.wikimedia.org/wikipedia/en/1/1c/Carolina_Panthers_logo.svg'),
    ('Chicago Bears',         'https://upload.wikimedia.org/wikipedia/commons/5/5c/Chicago_Bears_logo.svg'),
    ('Cincinnati Bengals',    'https://upload.wikimedia.org/wikipedia/commons/8/81/Cincinnati_Bengals_logo.svg'),
    ('Cleveland Browns',      'https://upload.wikimedia.org/wikipedia/en/d/d9/Cleveland_Browns_logo.svg'),
    ('Dallas Cowboys',        'https://upload.wikimedia.org/wikipedia/commons/1/15/Dallas_Cowboys.svg'),
    ('Denver Broncos',        'https://upload.wikimedia.org/wikipedia/en/4/44/Denver_Broncos_logo.svg'),
    ('Detroit Lions',         'https://upload.wikimedia.org/wikipedia/en/7/71/Detroit_Lions_logo.svg'),
    ('Green Bay Packers',     'https://upload.wikimedia.org/wikipedia/commons/5/50/Green_Bay_Packers_logo.svg'),
    ('Houston Texans',        'https://upload.wikimedia.org/wikipedia/en/2/28/Houston_Texans_logo.svg'),
    ('Indianapolis Colts',    'https://upload.wikimedia.org/wikipedia/commons/0/00/Indianapolis_Colts_logo.svg'),
    ('Jacksonville Jaguars',  'https://upload.wikimedia.org/wikipedia/en/7/74/Jacksonville_Jaguars_logo.svg'),
    ('Kansas City Chiefs',    'https://upload.wikimedia.org/wikipedia/en/e/e1/Kansas_City_Chiefs_logo.svg'),
    ('Las Vegas Raiders',     'https://upload.wikimedia.org/wikipedia/en/4/48/Las_Vegas_Raiders_logo.svg'),
    ('Los Angeles Chargers',  'https://upload.wikimedia.org/wikipedia/en/7/72/NFL_Chargers_logo.svg'),
    ('Los Angeles Rams',      'https://upload.wikimedia.org/wikipedia/en/8/8a/Los_Angeles_Rams_logo.svg'),
    ('Miami Dolphins',        'https://upload.wikimedia.org/wikipedia/en/3/37/Miami_Dolphins_logo.svg'),
    ('Minnesota Vikings',     'https://upload.wikimedia.org/wikipedia/en/4/48/Minnesota_Vikings_logo.svg'),
    ('New England Patriots',  'https://upload.wikimedia.org/wikipedia/en/b/b9/New_England_Patriots_logo.svg'),
    ('New Orleans Saints',    'https://upload.wikimedia.org/wikipedia/commons/5/50/New_Orleans_Saints_logo.svg'),
    ('New York Giants',       'https://upload.wikimedia.org/wikipedia/commons/6/60/New_York_Giants_logo.svg'),
    ('New York Jets',         'https://upload.wikimedia.org/wikipedia/en/6/6b/New_York_Jets_logo.svg'),
    ('Philadelphia Eagles',   'https://upload.wikimedia.org/wikipedia/en/8/8e/Philadelphia_Eagles_logo.svg'),
    ('Pittsburgh Steelers',   'https://upload.wikimedia.org/wikipedia/commons/d/de/Pittsburgh_Steelers_logo.svg'),
    ('San Francisco 49ers',   'https://upload.wikimedia.org/wikipedia/commons/3/3a/San_Francisco_49ers_logo.svg'),
    ('Seattle Seahawks',      'https://upload.wikimedia.org/wikipedia/en/8/8e/Seattle_Seahawks_logo.svg'),
    ('Tampa Bay Buccaneers',  'https://upload.wikimedia.org/wikipedia/en/a/a2/Tampa_Bay_Buccaneers_logo.svg'),
    ('Tennessee Titans',      'https://upload.wikimedia.org/wikipedia/en/5/53/Tennessee_Titans_Logo_2026.svg'),
    ('Washington Commanders', 'https://upload.wikimedia.org/wikipedia/commons/0/0c/Washington_Commanders_logo.svg')
) AS v(name, url)
WHERE t.sport = 'NFL' AND t.name = v.name;

-- Sanity: every NBA + NFL team must end with a Wikimedia source SVG path.
DO $$
DECLARE
    bad INT;
BEGIN
    SELECT COUNT(*) INTO bad
    FROM teams
    WHERE sport IN ('NBA','NFL')
      AND (logo_url IS NULL
           OR logo_url NOT LIKE 'https://upload.wikimedia.org/wikipedia/%.svg');
    IF bad > 0 THEN
        RAISE EXCEPTION 'Aborting: % NBA/NFL teams missing Wikimedia source SVG URL', bad;
    END IF;
END $$;

COMMIT;

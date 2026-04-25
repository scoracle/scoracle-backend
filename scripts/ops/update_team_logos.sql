-- Update NBA + NFL team logos from Wikimedia public-domain sources.
-- Source: /home/sheneveld/Downloads/nba_nfl_logos.md
-- Run: psql "$DATABASE_PRIVATE_URL" -f scripts/ops/update_team_logos.sql

BEGIN;

-- NBA --------------------------------------------------------------------
UPDATE teams t
SET logo_url = v.url,
    updated_at = NOW()
FROM (VALUES
    ('Atlanta Hawks',          'https://upload.wikimedia.org/wikipedia/en/thumb/2/24/Atlanta_Hawks_logo.svg/150px-Atlanta_Hawks_logo.svg.png'),
    ('Boston Celtics',         'https://upload.wikimedia.org/wikipedia/en/thumb/8/8f/Boston_Celtics.svg/150px-Boston_Celtics.svg.png'),
    ('Brooklyn Nets',          'https://upload.wikimedia.org/wikipedia/en/thumb/4/40/Brooklyn_Nets_primary_icon_logo_2024.svg/150px-Brooklyn_Nets_primary_icon_logo_2024.svg.png'),
    ('Charlotte Hornets',      'https://upload.wikimedia.org/wikipedia/en/thumb/c/c4/Charlotte_Hornets_(2014).svg/150px-Charlotte_Hornets_(2014).svg.png'),
    ('Chicago Bulls',          'https://upload.wikimedia.org/wikipedia/en/thumb/6/67/Chicago_Bulls_logo.svg/150px-Chicago_Bulls_logo.svg.png'),
    ('Cleveland Cavaliers',    'https://upload.wikimedia.org/wikipedia/commons/thumb/4/4b/Cleveland_Cavaliers_logo.svg/150px-Cleveland_Cavaliers_logo.svg.png'),
    ('Dallas Mavericks',       'https://upload.wikimedia.org/wikipedia/en/thumb/9/97/Dallas_Mavericks_logo.svg/150px-Dallas_Mavericks_logo.svg.png'),
    ('Denver Nuggets',         'https://upload.wikimedia.org/wikipedia/en/thumb/7/76/Denver_Nuggets.svg/150px-Denver_Nuggets.svg.png'),
    ('Detroit Pistons',        'https://upload.wikimedia.org/wikipedia/commons/thumb/c/c9/Logo_of_the_Detroit_Pistons.svg/150px-Logo_of_the_Detroit_Pistons.svg.png'),
    ('Golden State Warriors',  'https://upload.wikimedia.org/wikipedia/en/thumb/0/01/Golden_State_Warriors_logo.svg/150px-Golden_State_Warriors_logo.svg.png'),
    ('Houston Rockets',        'https://upload.wikimedia.org/wikipedia/en/thumb/2/28/Houston_Rockets.svg/150px-Houston_Rockets.svg.png'),
    ('Indiana Pacers',         'https://upload.wikimedia.org/wikipedia/en/thumb/1/1b/Indiana_Pacers.svg/150px-Indiana_Pacers.svg.png'),
    -- DB stores "LA Clippers"; markdown lists "Los Angeles Clippers".
    ('LA Clippers',            'https://upload.wikimedia.org/wikipedia/en/thumb/e/ed/Los_Angeles_Clippers_(2024).svg/150px-Los_Angeles_Clippers_(2024).svg.png'),
    ('Los Angeles Lakers',     'https://upload.wikimedia.org/wikipedia/commons/thumb/3/3c/Los_Angeles_Lakers_logo.svg/150px-Los_Angeles_Lakers_logo.svg.png'),
    ('Memphis Grizzlies',      'https://upload.wikimedia.org/wikipedia/en/thumb/f/f1/Memphis_Grizzlies.svg/150px-Memphis_Grizzlies.svg.png'),
    ('Miami Heat',             'https://upload.wikimedia.org/wikipedia/en/thumb/f/fb/Miami_Heat_logo.svg/150px-Miami_Heat_logo.svg.png'),
    ('Milwaukee Bucks',        'https://upload.wikimedia.org/wikipedia/en/thumb/4/4a/Milwaukee_Bucks_logo.svg/150px-Milwaukee_Bucks_logo.svg.png'),
    ('Minnesota Timberwolves', 'https://upload.wikimedia.org/wikipedia/en/thumb/c/c2/Minnesota_Timberwolves_logo.svg/150px-Minnesota_Timberwolves_logo.svg.png'),
    ('New Orleans Pelicans',   'https://upload.wikimedia.org/wikipedia/en/thumb/0/0d/New_Orleans_Pelicans_logo.svg/150px-New_Orleans_Pelicans_logo.svg.png'),
    ('New York Knicks',        'https://upload.wikimedia.org/wikipedia/en/thumb/2/25/New_York_Knicks_logo.svg/150px-New_York_Knicks_logo.svg.png'),
    ('Oklahoma City Thunder',  'https://upload.wikimedia.org/wikipedia/en/thumb/5/5d/Oklahoma_City_Thunder.svg/150px-Oklahoma_City_Thunder.svg.png'),
    ('Orlando Magic',          'https://upload.wikimedia.org/wikipedia/en/thumb/1/10/Orlando_Magic_logo.svg/150px-Orlando_Magic_logo.svg.png'),
    ('Philadelphia 76ers',     'https://upload.wikimedia.org/wikipedia/en/thumb/0/0e/Philadelphia_76ers_logo.svg/150px-Philadelphia_76ers_logo.svg.png'),
    ('Phoenix Suns',           'https://upload.wikimedia.org/wikipedia/en/thumb/d/dc/Phoenix_Suns_logo.svg/150px-Phoenix_Suns_logo.svg.png'),
    ('Portland Trail Blazers', 'https://upload.wikimedia.org/wikipedia/en/thumb/2/21/Portland_Trail_Blazers_logo.svg/150px-Portland_Trail_Blazers_logo.svg.png'),
    ('Sacramento Kings',       'https://upload.wikimedia.org/wikipedia/en/thumb/c/c7/SacramentoKings.svg/150px-SacramentoKings.svg.png'),
    ('San Antonio Spurs',      'https://upload.wikimedia.org/wikipedia/en/thumb/a/a2/San_Antonio_Spurs.svg/150px-San_Antonio_Spurs.svg.png'),
    ('Toronto Raptors',        'https://upload.wikimedia.org/wikipedia/en/thumb/3/36/Toronto_Raptors_logo.svg/150px-Toronto_Raptors_logo.svg.png'),
    ('Utah Jazz',              'https://upload.wikimedia.org/wikipedia/en/thumb/7/77/Utah_Jazz_logo_2025.svg/150px-Utah_Jazz_logo_2025.svg.png'),
    ('Washington Wizards',     'https://upload.wikimedia.org/wikipedia/en/thumb/0/02/Washington_Wizards_logo.svg/150px-Washington_Wizards_logo.svg.png')
) AS v(name, url)
WHERE t.sport = 'NBA' AND t.name = v.name;

-- NFL --------------------------------------------------------------------
UPDATE teams t
SET logo_url = v.url,
    updated_at = NOW()
FROM (VALUES
    ('Arizona Cardinals',     'https://upload.wikimedia.org/wikipedia/en/thumb/7/72/Arizona_Cardinals_logo.svg/150px-Arizona_Cardinals_logo.svg.png'),
    ('Atlanta Falcons',       'https://upload.wikimedia.org/wikipedia/en/thumb/c/c5/Atlanta_Falcons_logo.svg/150px-Atlanta_Falcons_logo.svg.png'),
    ('Baltimore Ravens',      'https://upload.wikimedia.org/wikipedia/en/thumb/1/16/Baltimore_Ravens_logo.svg/150px-Baltimore_Ravens_logo.svg.png'),
    ('Buffalo Bills',         'https://upload.wikimedia.org/wikipedia/en/thumb/7/77/Buffalo_Bills_logo.svg/150px-Buffalo_Bills_logo.svg.png'),
    ('Carolina Panthers',     'https://upload.wikimedia.org/wikipedia/en/thumb/1/1c/Carolina_Panthers_logo.svg/150px-Carolina_Panthers_logo.svg.png'),
    ('Chicago Bears',         'https://upload.wikimedia.org/wikipedia/commons/thumb/5/5c/Chicago_Bears_logo.svg/150px-Chicago_Bears_logo.svg.png'),
    ('Cincinnati Bengals',    'https://upload.wikimedia.org/wikipedia/commons/thumb/8/81/Cincinnati_Bengals_logo.svg/150px-Cincinnati_Bengals_logo.svg.png'),
    ('Cleveland Browns',      'https://upload.wikimedia.org/wikipedia/en/thumb/d/d9/Cleveland_Browns_logo.svg/150px-Cleveland_Browns_logo.svg.png'),
    ('Dallas Cowboys',        'https://upload.wikimedia.org/wikipedia/commons/thumb/1/15/Dallas_Cowboys.svg/150px-Dallas_Cowboys.svg.png'),
    ('Denver Broncos',        'https://upload.wikimedia.org/wikipedia/en/thumb/4/44/Denver_Broncos_logo.svg/150px-Denver_Broncos_logo.svg.png'),
    ('Detroit Lions',         'https://upload.wikimedia.org/wikipedia/en/thumb/7/71/Detroit_Lions_logo.svg/150px-Detroit_Lions_logo.svg.png'),
    ('Green Bay Packers',     'https://upload.wikimedia.org/wikipedia/commons/thumb/5/50/Green_Bay_Packers_logo.svg/150px-Green_Bay_Packers_logo.svg.png'),
    ('Houston Texans',        'https://upload.wikimedia.org/wikipedia/en/thumb/2/28/Houston_Texans_logo.svg/150px-Houston_Texans_logo.svg.png'),
    ('Indianapolis Colts',    'https://upload.wikimedia.org/wikipedia/commons/thumb/0/00/Indianapolis_Colts_logo.svg/150px-Indianapolis_Colts_logo.svg.png'),
    ('Jacksonville Jaguars',  'https://upload.wikimedia.org/wikipedia/en/thumb/7/74/Jacksonville_Jaguars_logo.svg/150px-Jacksonville_Jaguars_logo.svg.png'),
    ('Kansas City Chiefs',    'https://upload.wikimedia.org/wikipedia/en/thumb/e/e1/Kansas_City_Chiefs_logo.svg/150px-Kansas_City_Chiefs_logo.svg.png'),
    ('Las Vegas Raiders',     'https://upload.wikimedia.org/wikipedia/en/thumb/4/48/Las_Vegas_Raiders_logo.svg/150px-Las_Vegas_Raiders_logo.svg.png'),
    ('Los Angeles Chargers',  'https://upload.wikimedia.org/wikipedia/en/thumb/7/72/NFL_Chargers_logo.svg/150px-NFL_Chargers_logo.svg.png'),
    ('Los Angeles Rams',      'https://upload.wikimedia.org/wikipedia/en/thumb/8/8a/Los_Angeles_Rams_logo.svg/150px-Los_Angeles_Rams_logo.svg.png'),
    ('Miami Dolphins',        'https://upload.wikimedia.org/wikipedia/en/thumb/3/37/Miami_Dolphins_logo.svg/150px-Miami_Dolphins_logo.svg.png'),
    ('Minnesota Vikings',     'https://upload.wikimedia.org/wikipedia/en/thumb/4/48/Minnesota_Vikings_logo.svg/150px-Minnesota_Vikings_logo.svg.png'),
    ('New England Patriots',  'https://upload.wikimedia.org/wikipedia/en/thumb/b/b9/New_England_Patriots_logo.svg/150px-New_England_Patriots_logo.svg.png'),
    ('New Orleans Saints',    'https://upload.wikimedia.org/wikipedia/commons/thumb/5/50/New_Orleans_Saints_logo.svg/150px-New_Orleans_Saints_logo.svg.png'),
    ('New York Giants',       'https://upload.wikimedia.org/wikipedia/commons/thumb/6/60/New_York_Giants_logo.svg/150px-New_York_Giants_logo.svg.png'),
    ('New York Jets',         'https://upload.wikimedia.org/wikipedia/en/thumb/6/6b/New_York_Jets_logo.svg/150px-New_York_Jets_logo.svg.png'),
    ('Philadelphia Eagles',   'https://upload.wikimedia.org/wikipedia/en/thumb/8/8e/Philadelphia_Eagles_logo.svg/150px-Philadelphia_Eagles_logo.svg.png'),
    ('Pittsburgh Steelers',   'https://upload.wikimedia.org/wikipedia/commons/thumb/d/de/Pittsburgh_Steelers_logo.svg/150px-Pittsburgh_Steelers_logo.svg.png'),
    ('San Francisco 49ers',   'https://upload.wikimedia.org/wikipedia/commons/thumb/3/3a/San_Francisco_49ers_logo.svg/150px-San_Francisco_49ers_logo.svg.png'),
    ('Seattle Seahawks',      'https://upload.wikimedia.org/wikipedia/en/thumb/8/8e/Seattle_Seahawks_logo.svg/150px-Seattle_Seahawks_logo.svg.png'),
    ('Tampa Bay Buccaneers',  'https://upload.wikimedia.org/wikipedia/en/thumb/a/a2/Tampa_Bay_Buccaneers_logo.svg/150px-Tampa_Bay_Buccaneers_logo.svg.png'),
    ('Tennessee Titans',      'https://upload.wikimedia.org/wikipedia/en/thumb/5/53/Tennessee_Titans_Logo_2026.svg/150px-Tennessee_Titans_Logo_2026.svg.png'),
    ('Washington Commanders', 'https://upload.wikimedia.org/wikipedia/commons/thumb/0/0c/Washington_Commanders_logo.svg/150px-Washington_Commanders_logo.svg.png')
) AS v(name, url)
WHERE t.sport = 'NFL' AND t.name = v.name;

-- Sanity: every NBA + NFL team must end with a logo from upload.wikimedia.org.
DO $$
DECLARE
    missing_count INT;
BEGIN
    SELECT COUNT(*) INTO missing_count
    FROM teams
    WHERE sport IN ('NBA','NFL')
      AND (logo_url IS NULL OR logo_url NOT LIKE 'https://upload.wikimedia.org/%');
    IF missing_count > 0 THEN
        RAISE EXCEPTION 'Aborting: % NBA/NFL teams missing wikimedia logo_url', missing_count;
    END IF;
END $$;

COMMIT;

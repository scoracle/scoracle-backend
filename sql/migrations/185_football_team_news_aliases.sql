-- 185_football_team_news_aliases.sql
--
-- Backfill richer FOOTBALL team aliases for the Google News RSS corpus sweep.
-- The RSS code now fans team searches out through alias OR-groups and major
-- European Google News editions; this gives that wider net useful club names
-- instead of relying mostly on provider short codes.

BEGIN;

CREATE TEMP TABLE tmp_football_team_news_aliases (
    team_name text PRIMARY KEY,
    aliases text[] NOT NULL
) ON COMMIT DROP;

INSERT INTO tmp_football_team_news_aliases (team_name, aliases) VALUES
    ('AFC Bournemouth',          ARRAY['Bournemouth', 'AFC Bournemouth']::text[]),
    ('Arsenal',                  ARRAY['Arsenal FC', 'The Gunners', 'Gunners']),
    ('Aston Villa',              ARRAY['Aston Villa FC', 'Villa', 'AVFC']),
    ('Brentford',                ARRAY['Brentford FC', 'The Bees']),
    ('Brighton & Hove Albion',   ARRAY['Brighton and Hove Albion', 'Brighton Hove Albion', 'Brighton', 'BHAFC']),
    ('Burnley',                  ARRAY['Burnley FC', 'The Clarets']),
    ('Chelsea',                  ARRAY['Chelsea FC', 'The Blues']),
    ('Crystal Palace',           ARRAY['Crystal Palace FC', 'Palace', 'The Eagles']),
    ('Everton',                  ARRAY['Everton FC', 'The Toffees']),
    ('Fulham',                   ARRAY['Fulham FC', 'The Cottagers']),
    ('Ipswich Town',             ARRAY['Ipswich', 'Ipswich Town FC']),
    ('Leeds United',             ARRAY['Leeds', 'Leeds United FC']),
    ('Leicester City',           ARRAY['Leicester', 'Leicester City FC']),
    ('Liverpool',                ARRAY['Liverpool FC', 'LFC', 'The Reds']),
    ('Luton Town',               ARRAY['Luton', 'Luton Town FC']),
    ('Manchester City',          ARRAY['Manchester City FC', 'Man City', 'MCFC']),
    ('Manchester United',        ARRAY['Manchester United FC', 'Man United', 'Man UTD', 'Man Utd', 'Man U', 'MUFC']),
    ('Newcastle United',         ARRAY['Newcastle', 'Newcastle United FC', 'NUFC']),
    ('Nottingham Forest',        ARRAY['Nottingham Forest FC', 'Nottm Forest', 'Nott''m Forest', 'Forest']),
    ('Sheffield United',         ARRAY['Sheffield Utd', 'Sheff Utd', 'Sheffield United FC']),
    ('Southampton',              ARRAY['Southampton FC', 'The Saints']),
    ('Sunderland',               ARRAY['Sunderland AFC', 'Sunderland FC']),
    ('Tottenham Hotspur',        ARRAY['Tottenham', 'Spurs', 'Tottenham Hotspur FC', 'THFC']),
    ('Watford',                  ARRAY['Watford FC']),
    ('West Bromwich Albion',     ARRAY['West Brom', 'West Bromwich', 'West Bromwich Albion FC', 'WBA']),
    ('West Ham United',          ARRAY['West Ham', 'West Ham United FC', 'WHUFC']),
    ('Wolverhampton Wanderers',  ARRAY['Wolves', 'Wolverhampton', 'Wolverhampton Wanderers FC']),

    ('Bayer 04 Leverkusen',      ARRAY['Bayer Leverkusen', 'Leverkusen', 'Bayer 04', 'B04']),
    ('Borussia Dortmund',        ARRAY['Dortmund', 'BVB', 'BVB Dortmund']),
    ('Borussia Mönchengladbach', ARRAY['Borussia Monchengladbach', 'Borussia M''gladbach', 'Gladbach', 'Mönchengladbach', 'Monchengladbach', 'BMG']),
    ('Darmstadt 98',             ARRAY['SV Darmstadt 98', 'Darmstadt']),
    ('DSC Arminia Bielefeld',    ARRAY['Arminia Bielefeld', 'Bielefeld']),
    ('Eintracht Frankfurt',      ARRAY['Frankfurt', 'SGE']),
    ('FC Augsburg',              ARRAY['Augsburg']),
    ('FC Bayern München',        ARRAY['Bayern Munich', 'Bayern Munchen', 'Bayern München', 'Bayern', 'FC Bayern', 'Bayern Monaco']),
    ('FC Köln',                  ARRAY['1. FC Köln', '1. FC Koln', 'FC Koln', 'Koln', 'Cologne']),
    ('FC Union Berlin',          ARRAY['Union Berlin', '1. FC Union Berlin']),
    ('Fortuna Düsseldorf',       ARRAY['Fortuna Dusseldorf', 'Fortuna Düsseldorf', 'Fortuna']),
    ('FSV Mainz 05',             ARRAY['Mainz 05', 'Mainz']),
    ('Hamburger SV',             ARRAY['Hamburg', 'HSV']),
    ('Heidenheim',               ARRAY['1. FC Heidenheim', 'FC Heidenheim']),
    ('Hertha BSC',               ARRAY['Hertha Berlin', 'Hertha']),
    ('Holstein Kiel',            ARRAY['Kiel', 'Holstein Kiel']),
    ('Paderborn',                ARRAY['SC Paderborn', 'Paderborn 07']),
    ('RB Leipzig',               ARRAY['Leipzig', 'RasenBallsport Leipzig', 'RBL']),
    ('SC Freiburg',              ARRAY['Freiburg']),
    ('Schalke 04',               ARRAY['FC Schalke 04', 'Schalke', 'S04']),
    ('SpVgg Greuther Fürth',     ARRAY['Greuther Furth', 'Greuther Fürth', 'Furth', 'Fürth']),
    ('St. Pauli',                ARRAY['FC St. Pauli', 'St Pauli']),
    ('TSG Hoffenheim',           ARRAY['Hoffenheim']),
    ('VfB Stuttgart',            ARRAY['Stuttgart']),
    ('VfL Bochum 1848',          ARRAY['VfL Bochum', 'Bochum 1848', 'Bochum']),
    ('VfL Wolfsburg',            ARRAY['Wolfsburg']),
    ('Werder Bremen',            ARRAY['SV Werder Bremen', 'Bremen']),

    ('Ajaccio',                  ARRAY['AC Ajaccio']),
    ('Amiens SC',                ARRAY['Amiens']),
    ('Angers SCO',               ARRAY['Angers']),
    ('Auxerre',                  ARRAY['AJ Auxerre']),
    ('Bordeaux',                 ARRAY['Girondins de Bordeaux', 'Girondins Bordeaux']),
    ('Brest',                    ARRAY['Stade Brestois', 'Stade Brestois 29']),
    ('Clermont',                 ARRAY['Clermont Foot', 'Clermont Foot 63']),
    ('Dijon',                    ARRAY['Dijon FCO']),
    ('Le Havre',                 ARRAY['Le Havre AC', 'HAC']),
    ('Lens',                     ARRAY['RC Lens', 'Racing Club de Lens']),
    ('Lorient',                  ARRAY['FC Lorient']),
    ('LOSC Lille',               ARRAY['Lille', 'Lille OSC', 'LOSC']),
    ('Metz',                     ARRAY['FC Metz']),
    ('Monaco',                   ARRAY['AS Monaco', 'ASM']),
    ('Montpellier',              ARRAY['Montpellier HSC', 'MHSC']),
    ('Nantes',                   ARRAY['FC Nantes']),
    ('Nice',                     ARRAY['OGC Nice']),
    ('Nîmes',                    ARRAY['Nimes', 'Nîmes Olympique', 'Nimes Olympique']),
    ('Olympique Lyonnais',       ARRAY['Lyon', 'OL', 'Olympique Lyon']),
    ('Olympique Marseille',      ARRAY['Marseille', 'OM', 'Olympique de Marseille']),
    ('Paris',                    ARRAY['Paris FC', 'PFC']),
    ('Paris Saint Germain',      ARRAY['Paris Saint-Germain', 'Paris SG', 'PSG']),
    ('Reims',                    ARRAY['Stade de Reims']),
    ('Rennes',                   ARRAY['Stade Rennais', 'Stade Rennais FC']),
    ('Saint-Étienne',            ARRAY['Saint Etienne', 'Saint-Étienne', 'AS Saint-Etienne', 'AS Saint-Étienne', 'ASSE']),
    ('Strasbourg',               ARRAY['RC Strasbourg', 'Racing Strasbourg']),
    ('Toulouse',                 ARRAY['Toulouse FC']),
    ('Troyes',                   ARRAY['ESTAC Troyes', 'ESTAC']),

    ('AC Milan',                 ARRAY['Milan', 'AC Milan', 'Milan AC', 'AC Mailand']),
    ('Atalanta',                 ARRAY['Atalanta BC']),
    ('Benevento',                ARRAY['Benevento Calcio']),
    ('Bologna',                  ARRAY['Bologna FC', 'Bologna FC 1909']),
    ('Brescia',                  ARRAY['Brescia Calcio']),
    ('Cagliari',                 ARRAY['Cagliari Calcio']),
    ('Como',                     ARRAY['Como 1907']),
    ('Cremonese',                ARRAY['US Cremonese']),
    ('Crotone',                  ARRAY['FC Crotone']),
    ('Empoli',                   ARRAY['Empoli FC']),
    ('Fiorentina',               ARRAY['ACF Fiorentina', 'Viola']),
    ('Frosinone',                ARRAY['Frosinone Calcio']),
    ('Genoa',                    ARRAY['Genoa CFC']),
    ('Hellas Verona',            ARRAY['Verona', 'Hellas Verona FC']),
    ('Inter',                    ARRAY['Inter Milan', 'Internazionale', 'FC Internazionale', 'Internazionale Milano', 'Inter de Milan', 'Inter Mailand']),
    ('Juventus',                 ARRAY['Juve', 'Juventus FC', 'Juventus Turin', 'Juventus Torino']),
    ('Lazio',                    ARRAY['SS Lazio', 'Lazio Rome']),
    ('Lecce',                    ARRAY['US Lecce']),
    ('Monza',                    ARRAY['AC Monza']),
    ('Napoli',                   ARRAY['SSC Napoli', 'Naples']),
    ('Parma',                    ARRAY['Parma Calcio']),
    ('Pisa',                     ARRAY['Pisa SC', 'Pisa Sporting Club']),
    ('Roma',                     ARRAY['AS Roma', 'Roma FC']),
    ('Salernitana',              ARRAY['US Salernitana']),
    ('Sampdoria',                ARRAY['UC Sampdoria']),
    ('Sassuolo',                 ARRAY['US Sassuolo']),
    ('SPAL',                     ARRAY['SPAL Ferrara']),
    ('Spezia',                   ARRAY['Spezia Calcio']),
    ('Torino',                   ARRAY['Torino FC']),
    ('Udinese',                  ARRAY['Udinese Calcio']),
    ('Venezia',                  ARRAY['Venezia FC']),

    ('Almería',                  ARRAY['Almeria', 'UD Almería', 'UD Almeria']),
    ('Athletic Club',            ARRAY['Athletic Bilbao', 'Athletic Club Bilbao']),
    ('Atlético de Madrid',       ARRAY['Atletico de Madrid', 'Atlético Madrid', 'Atletico Madrid', 'Atleti']),
    ('Cádiz',                    ARRAY['Cadiz', 'Cádiz CF', 'Cadiz CF']),
    ('Celta de Vigo',            ARRAY['Celta Vigo', 'Celta', 'RC Celta']),
    ('Deportivo Alavés',         ARRAY['Deportivo Alaves', 'Alaves', 'Alavés']),
    ('Elche',                    ARRAY['Elche CF']),
    ('Espanyol',                 ARRAY['RCD Espanyol']),
    ('FC Barcelona',             ARRAY['Barcelona', 'Barca', 'Barça', 'FC Barcelona']),
    ('Getafe',                   ARRAY['Getafe CF']),
    ('Girona',                   ARRAY['Girona FC']),
    ('Granada',                  ARRAY['Granada CF']),
    ('Huesca',                   ARRAY['SD Huesca']),
    ('Las Palmas',               ARRAY['UD Las Palmas']),
    ('Leganés',                  ARRAY['Leganes', 'CD Leganés', 'CD Leganes']),
    ('Levante',                  ARRAY['Levante UD']),
    ('Mallorca',                 ARRAY['RCD Mallorca']),
    ('Osasuna',                  ARRAY['CA Osasuna']),
    ('Rayo Vallecano',           ARRAY['Rayo', 'Rayo Vallecano de Madrid']),
    ('Real Betis',               ARRAY['Betis', 'Real Betis Balompie', 'Real Betis Balompié']),
    ('Real Madrid',              ARRAY['Real Madrid CF', 'Madrid', 'Los Blancos']),
    ('Real Oviedo',              ARRAY['Oviedo']),
    ('Real Sociedad',            ARRAY['Real Sociedad de Fútbol', 'Real Sociedad de Futbol']),
    ('Real Valladolid',          ARRAY['Valladolid', 'Real Valladolid CF']),
    ('SD Eibar',                 ARRAY['Eibar']),
    ('Sevilla',                  ARRAY['Sevilla FC', 'Seville']),
    ('Valencia',                 ARRAY['Valencia CF']),
    ('Villarreal',               ARRAY['Villarreal CF']);

WITH merged AS (
    SELECT t.id, t.sport,
           ARRAY(
               SELECT alias
               FROM (
                   SELECT DISTINCT ON (lower(alias)) alias, ord
                   FROM (
                       SELECT btrim(a) AS alias, ord::int AS ord
                       FROM unnest(COALESCE(t.search_aliases, ARRAY[]::text[])) WITH ORDINALITY AS e(a, ord)
                       UNION ALL
                       SELECT btrim(a) AS alias, (1000 + ord)::int AS ord
                       FROM unnest(s.aliases) WITH ORDINALITY AS n(a, ord)
                   ) all_aliases
                   WHERE alias <> ''
                   ORDER BY lower(alias), ord
               ) deduped
               ORDER BY ord
           ) AS aliases
    FROM public.teams t
    JOIN tmp_football_team_news_aliases s ON s.team_name = t.name
    WHERE t.sport = 'FOOTBALL'
)
UPDATE public.teams t
   SET search_aliases = m.aliases,
       updated_at = NOW()
  FROM merged m
 WHERE t.id = m.id
   AND t.sport = m.sport
   AND t.search_aliases IS DISTINCT FROM m.aliases;

DO $$
DECLARE
    missing text[];
BEGIN
    SELECT array_agg(s.team_name ORDER BY s.team_name)
      INTO missing
      FROM tmp_football_team_news_aliases s
     WHERE NOT EXISTS (
         SELECT 1 FROM public.teams t
          WHERE t.sport = 'FOOTBALL' AND t.name = s.team_name
     );

    IF COALESCE(array_length(missing, 1), 0) > 0 THEN
        RAISE EXCEPTION 'football alias backfill target(s) missing: %', missing;
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.teams
         WHERE sport = 'FOOTBALL'
           AND name = 'Manchester United'
           AND search_aliases @> ARRAY['Man UTD', 'Man United', 'MUFC']::text[]
    ) THEN
        RAISE EXCEPTION 'football alias backfill failed for Manchester United';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM public.teams
         WHERE sport = 'FOOTBALL'
           AND name = 'Paris Saint Germain'
           AND search_aliases @> ARRAY['PSG', 'Paris Saint-Germain', 'Paris SG']::text[]
    ) THEN
        RAISE EXCEPTION 'football alias backfill failed for Paris Saint Germain';
    END IF;
END $$;

INSERT INTO public.schema_migrations(version) VALUES ('185_football_team_news_aliases')
    ON CONFLICT DO NOTHING;

COMMIT;

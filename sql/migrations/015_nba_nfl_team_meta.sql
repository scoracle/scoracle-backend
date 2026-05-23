-- Backfill basic team metadata for NBA and NFL (founded, venue_name,
-- venue_capacity, country). Football already has these populated via the
-- SportMonks ingest; NBA/NFL come from BallDontLie which only ships
-- conference/division/city/logo, so the canonical profile payload has been
-- sparse for these fields.
--
-- Values are well-known public facts (current franchise founding year,
-- current primary home venue, listed capacity at that venue). After the
-- backfill we refresh the autofill materialized views since their team
-- rows embed founded / venue_name / venue_capacity in the meta JSON.

BEGIN;

-- ============================================================================
-- NBA — 30 teams. Founded = current franchise founding year (relocations
-- preserve the franchise; new expansion teams use their inaugural year).
-- All US-based except Toronto (Canada).
-- ============================================================================

UPDATE teams SET founded = 1946, venue_name = 'State Farm Arena',          venue_capacity = 16888, country = 'United States' WHERE sport = 'NBA' AND id =  1; -- Atlanta Hawks
UPDATE teams SET founded = 1946, venue_name = 'TD Garden',                 venue_capacity = 19156, country = 'United States' WHERE sport = 'NBA' AND id =  2; -- Boston Celtics
UPDATE teams SET founded = 1967, venue_name = 'Barclays Center',           venue_capacity = 17732, country = 'United States' WHERE sport = 'NBA' AND id =  3; -- Brooklyn Nets
UPDATE teams SET founded = 1988, venue_name = 'Spectrum Center',           venue_capacity = 19077, country = 'United States' WHERE sport = 'NBA' AND id =  4; -- Charlotte Hornets
UPDATE teams SET founded = 1966, venue_name = 'United Center',             venue_capacity = 20917, country = 'United States' WHERE sport = 'NBA' AND id =  5; -- Chicago Bulls
UPDATE teams SET founded = 1970, venue_name = 'Rocket Arena',              venue_capacity = 19432, country = 'United States' WHERE sport = 'NBA' AND id =  6; -- Cleveland Cavaliers
UPDATE teams SET founded = 1980, venue_name = 'American Airlines Center',  venue_capacity = 19200, country = 'United States' WHERE sport = 'NBA' AND id =  7; -- Dallas Mavericks
UPDATE teams SET founded = 1967, venue_name = 'Ball Arena',                venue_capacity = 19520, country = 'United States' WHERE sport = 'NBA' AND id =  8; -- Denver Nuggets
UPDATE teams SET founded = 1941, venue_name = 'Little Caesars Arena',      venue_capacity = 20332, country = 'United States' WHERE sport = 'NBA' AND id =  9; -- Detroit Pistons
UPDATE teams SET founded = 1946, venue_name = 'Chase Center',              venue_capacity = 18064, country = 'United States' WHERE sport = 'NBA' AND id = 10; -- Golden State Warriors
UPDATE teams SET founded = 1967, venue_name = 'Toyota Center',             venue_capacity = 18055, country = 'United States' WHERE sport = 'NBA' AND id = 11; -- Houston Rockets
UPDATE teams SET founded = 1967, venue_name = 'Gainbridge Fieldhouse',     venue_capacity = 17923, country = 'United States' WHERE sport = 'NBA' AND id = 12; -- Indiana Pacers
UPDATE teams SET founded = 1970, venue_name = 'Intuit Dome',               venue_capacity = 18000, country = 'United States' WHERE sport = 'NBA' AND id = 13; -- LA Clippers
UPDATE teams SET founded = 1947, venue_name = 'Crypto.com Arena',          venue_capacity = 18997, country = 'United States' WHERE sport = 'NBA' AND id = 14; -- Los Angeles Lakers
UPDATE teams SET founded = 1995, venue_name = 'FedExForum',                venue_capacity = 18119, country = 'United States' WHERE sport = 'NBA' AND id = 15; -- Memphis Grizzlies
UPDATE teams SET founded = 1988, venue_name = 'Kaseya Center',             venue_capacity = 19600, country = 'United States' WHERE sport = 'NBA' AND id = 16; -- Miami Heat
UPDATE teams SET founded = 1968, venue_name = 'Fiserv Forum',              venue_capacity = 17341, country = 'United States' WHERE sport = 'NBA' AND id = 17; -- Milwaukee Bucks
UPDATE teams SET founded = 1989, venue_name = 'Target Center',             venue_capacity = 18978, country = 'United States' WHERE sport = 'NBA' AND id = 18; -- Minnesota Timberwolves
UPDATE teams SET founded = 2002, venue_name = 'Smoothie King Center',      venue_capacity = 16867, country = 'United States' WHERE sport = 'NBA' AND id = 19; -- New Orleans Pelicans
UPDATE teams SET founded = 1946, venue_name = 'Madison Square Garden',     venue_capacity = 19812, country = 'United States' WHERE sport = 'NBA' AND id = 20; -- New York Knicks
UPDATE teams SET founded = 1967, venue_name = 'Paycom Center',             venue_capacity = 18203, country = 'United States' WHERE sport = 'NBA' AND id = 21; -- Oklahoma City Thunder
UPDATE teams SET founded = 1989, venue_name = 'Kia Center',                venue_capacity = 18846, country = 'United States' WHERE sport = 'NBA' AND id = 22; -- Orlando Magic
UPDATE teams SET founded = 1946, venue_name = 'Wells Fargo Center',        venue_capacity = 20478, country = 'United States' WHERE sport = 'NBA' AND id = 23; -- Philadelphia 76ers
UPDATE teams SET founded = 1968, venue_name = 'Mortgage Matchup Center',   venue_capacity = 17071, country = 'United States' WHERE sport = 'NBA' AND id = 24; -- Phoenix Suns
UPDATE teams SET founded = 1970, venue_name = 'Moda Center',               venue_capacity = 19393, country = 'United States' WHERE sport = 'NBA' AND id = 25; -- Portland Trail Blazers
UPDATE teams SET founded = 1923, venue_name = 'Golden 1 Center',           venue_capacity = 17608, country = 'United States' WHERE sport = 'NBA' AND id = 26; -- Sacramento Kings
UPDATE teams SET founded = 1967, venue_name = 'Frost Bank Center',         venue_capacity = 18418, country = 'United States' WHERE sport = 'NBA' AND id = 27; -- San Antonio Spurs
UPDATE teams SET founded = 1995, venue_name = 'Scotiabank Arena',          venue_capacity = 19800, country = 'Canada'        WHERE sport = 'NBA' AND id = 28; -- Toronto Raptors
UPDATE teams SET founded = 1974, venue_name = 'Delta Center',              venue_capacity = 18306, country = 'United States' WHERE sport = 'NBA' AND id = 29; -- Utah Jazz
UPDATE teams SET founded = 1961, venue_name = 'Capital One Arena',         venue_capacity = 20356, country = 'United States' WHERE sport = 'NBA' AND id = 30; -- Washington Wizards


-- ============================================================================
-- NFL — 32 teams. Founded = franchise inception year (AFL franchises use
-- 1960; pre-NFL APFA franchises use their actual founding year).
-- All US-based.
-- ============================================================================

UPDATE teams SET founded = 1959, venue_name = 'Gillette Stadium',                       venue_capacity = 65878, country = 'United States' WHERE sport = 'NFL' AND id =  1; -- New England Patriots
UPDATE teams SET founded = 1960, venue_name = 'Highmark Stadium',                       venue_capacity = 71608, country = 'United States' WHERE sport = 'NFL' AND id =  3; -- Buffalo Bills
UPDATE teams SET founded = 1959, venue_name = 'MetLife Stadium',                        venue_capacity = 82500, country = 'United States' WHERE sport = 'NFL' AND id =  4; -- New York Jets
UPDATE teams SET founded = 1966, venue_name = 'Hard Rock Stadium',                      venue_capacity = 65326, country = 'United States' WHERE sport = 'NFL' AND id =  5; -- Miami Dolphins
UPDATE teams SET founded = 1996, venue_name = 'M&T Bank Stadium',                       venue_capacity = 71008, country = 'United States' WHERE sport = 'NFL' AND id =  6; -- Baltimore Ravens
UPDATE teams SET founded = 1933, venue_name = 'Acrisure Stadium',                       venue_capacity = 68400, country = 'United States' WHERE sport = 'NFL' AND id =  7; -- Pittsburgh Steelers
UPDATE teams SET founded = 1946, venue_name = 'Huntington Bank Field',                  venue_capacity = 67895, country = 'United States' WHERE sport = 'NFL' AND id =  8; -- Cleveland Browns
UPDATE teams SET founded = 1968, venue_name = 'Paycor Stadium',                         venue_capacity = 65515, country = 'United States' WHERE sport = 'NFL' AND id =  9; -- Cincinnati Bengals
UPDATE teams SET founded = 2002, venue_name = 'NRG Stadium',                            venue_capacity = 72220, country = 'United States' WHERE sport = 'NFL' AND id = 10; -- Houston Texans
UPDATE teams SET founded = 1960, venue_name = 'Nissan Stadium',                         venue_capacity = 69143, country = 'United States' WHERE sport = 'NFL' AND id = 11; -- Tennessee Titans
UPDATE teams SET founded = 1953, venue_name = 'Lucas Oil Stadium',                      venue_capacity = 67000, country = 'United States' WHERE sport = 'NFL' AND id = 12; -- Indianapolis Colts
UPDATE teams SET founded = 1995, venue_name = 'EverBank Stadium',                       venue_capacity = 67814, country = 'United States' WHERE sport = 'NFL' AND id = 13; -- Jacksonville Jaguars
UPDATE teams SET founded = 1960, venue_name = 'GEHA Field at Arrowhead Stadium',       venue_capacity = 76416, country = 'United States' WHERE sport = 'NFL' AND id = 14; -- Kansas City Chiefs
UPDATE teams SET founded = 1960, venue_name = 'Empower Field at Mile High',             venue_capacity = 76125, country = 'United States' WHERE sport = 'NFL' AND id = 15; -- Denver Broncos
UPDATE teams SET founded = 1960, venue_name = 'Allegiant Stadium',                      venue_capacity = 65000, country = 'United States' WHERE sport = 'NFL' AND id = 16; -- Las Vegas Raiders
UPDATE teams SET founded = 1960, venue_name = 'SoFi Stadium',                           venue_capacity = 70240, country = 'United States' WHERE sport = 'NFL' AND id = 17; -- Los Angeles Chargers
UPDATE teams SET founded = 1933, venue_name = 'Lincoln Financial Field',                venue_capacity = 69879, country = 'United States' WHERE sport = 'NFL' AND id = 18; -- Philadelphia Eagles
UPDATE teams SET founded = 1960, venue_name = 'AT&T Stadium',                           venue_capacity = 80000, country = 'United States' WHERE sport = 'NFL' AND id = 19; -- Dallas Cowboys
UPDATE teams SET founded = 1925, venue_name = 'MetLife Stadium',                        venue_capacity = 82500, country = 'United States' WHERE sport = 'NFL' AND id = 20; -- New York Giants
UPDATE teams SET founded = 1932, venue_name = 'Northwest Stadium',                      venue_capacity = 67617, country = 'United States' WHERE sport = 'NFL' AND id = 21; -- Washington Commanders
UPDATE teams SET founded = 1919, venue_name = 'Lambeau Field',                          venue_capacity = 81441, country = 'United States' WHERE sport = 'NFL' AND id = 22; -- Green Bay Packers
UPDATE teams SET founded = 1961, venue_name = 'U.S. Bank Stadium',                      venue_capacity = 66860, country = 'United States' WHERE sport = 'NFL' AND id = 23; -- Minnesota Vikings
UPDATE teams SET founded = 1919, venue_name = 'Soldier Field',                          venue_capacity = 61500, country = 'United States' WHERE sport = 'NFL' AND id = 24; -- Chicago Bears
UPDATE teams SET founded = 1930, venue_name = 'Ford Field',                             venue_capacity = 65000, country = 'United States' WHERE sport = 'NFL' AND id = 25; -- Detroit Lions
UPDATE teams SET founded = 1967, venue_name = 'Caesars Superdome',                      venue_capacity = 73208, country = 'United States' WHERE sport = 'NFL' AND id = 26; -- New Orleans Saints
UPDATE teams SET founded = 1966, venue_name = 'Mercedes-Benz Stadium',                  venue_capacity = 71000, country = 'United States' WHERE sport = 'NFL' AND id = 27; -- Atlanta Falcons
UPDATE teams SET founded = 1976, venue_name = 'Raymond James Stadium',                  venue_capacity = 65890, country = 'United States' WHERE sport = 'NFL' AND id = 28; -- Tampa Bay Buccaneers
UPDATE teams SET founded = 1995, venue_name = 'Bank of America Stadium',                venue_capacity = 74867, country = 'United States' WHERE sport = 'NFL' AND id = 29; -- Carolina Panthers
UPDATE teams SET founded = 1946, venue_name = 'Levi''s Stadium',                        venue_capacity = 68500, country = 'United States' WHERE sport = 'NFL' AND id = 30; -- San Francisco 49ers
UPDATE teams SET founded = 1976, venue_name = 'Lumen Field',                            venue_capacity = 68740, country = 'United States' WHERE sport = 'NFL' AND id = 31; -- Seattle Seahawks
UPDATE teams SET founded = 1936, venue_name = 'SoFi Stadium',                           venue_capacity = 70240, country = 'United States' WHERE sport = 'NFL' AND id = 32; -- Los Angeles Rams

-- Fix broken Rams logo URL (Wikipedia moved the file when the team
-- rebranded from "Los_Angeles_Rams_logo.svg" to "LA_Rams_logo.svg").
UPDATE teams SET logo_url = 'https://upload.wikimedia.org/wikipedia/en/1/14/LA_Rams_logo.svg' WHERE sport = 'NFL' AND id = 32;
UPDATE teams SET founded = 1898, venue_name = 'State Farm Stadium',                     venue_capacity = 63400, country = 'United States' WHERE sport = 'NFL' AND id = 33; -- Arizona Cardinals

-- Bump updated_at so any cache-versioned consumer sees the change.
UPDATE teams SET updated_at = NOW() WHERE sport IN ('NBA', 'NFL');

COMMIT;

-- Refresh the autofill materialized views — their `team` rows embed
-- founded / venue_name / venue_capacity in the JSON meta and won't see
-- the backfill until refreshed.
REFRESH MATERIALIZED VIEW CONCURRENTLY nba.autofill_entities;
REFRESH MATERIALIZED VIEW CONCURRENTLY nfl.autofill_entities;

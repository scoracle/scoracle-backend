-- 007: Convert height/weight columns to text, rename for imperial storage
--
-- The backend stores raw provider values as text:
--   NBA/NFL (BallDontLie): height = "6-6", weight = "225"  (already imperial)
--   Football (SportMonks):  height = "6-1", weight = "176"  (converted from metric)
-- The frontend handles display formatting based on sport context.

ALTER TABLE players
    ALTER COLUMN height_cm TYPE text USING height_cm::text,
    ALTER COLUMN weight_kg TYPE text USING weight_kg::text;

ALTER TABLE players RENAME COLUMN height_cm TO height;
ALTER TABLE players RENAME COLUMN weight_kg TO weight;

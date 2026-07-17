-- 153_sigil_voice_copy_freeze.sql
--
-- Session C of the sigil pipeline cleanup plan (2026-07-16): entity_sigil serving
-- moves to the merged sigil_synthesis voice columns (mig 152) and oracle_readings
-- freezes as read-only history.
--
-- DATA-ONLY. One-time copy (Scott's pick, Session C decision 1a): each entity's
-- latest real oracle_readings reading lands in the sigil_synthesis row that
-- serving actually picks, so entity_sigil reads merged columns ONLY — no COALESCE
-- fallback to the frozen table. Without this copy, serve-latest (North Star #7)
-- would keep a pre-merge reading invisible until the entity's first post-merge
-- voice, and quiet entities might never re-voice (North Star #8 triggers only on
-- omen flip / archetype band cross / first reading).
--
-- Copy semantics mirror the entity_sigil query's serving scopes exactly:
--   LIVE view  — per (sport, entity): the latest scored row where season is the
--                sport's current season OR NULL (legacy pre-S12 rows), receiving
--                the latest real CURRENT-season reading.
--   HISTORICAL — per (sport, entity, season <> current): the latest scored row of
--                that exact season, receiving that season's latest real reading.
--                (Measured 0 rows on prod 2026-07-16 — kept for replay correctness.)
-- Only rows whose voice columns are still empty are touched (reading IS NULL):
-- post-merge voices (Session B) always win over the copy.
--
-- Measured on prod before authoring (2026-07-16): 2,596 servable live-view
-- entities; 1,298 copy targets; 1,297 with no reading anywhere (they serve a
-- voiceless card today too — no regression); 1 already voiced post-merge.

BEGIN;

-- LIVE view copy.
WITH live_latest AS (
    SELECT DISTINCT ON (ss.sport, ss.entity_type, ss.entity_id)
           ss.id, ss.sport, ss.entity_type, ss.entity_id
    FROM public.sigil_synthesis ss
    JOIN public.sports sp ON sp.id = ss.sport
    WHERE ss.score IS NOT NULL
      AND (ss.season = sp.current_season OR ss.season IS NULL)
    ORDER BY ss.sport, ss.entity_type, ss.entity_id, ss.generated_at DESC
),
live_reading AS (
    SELECT DISTINCT ON (o.sport, o.entity_type, o.entity_id)
           o.sport, o.entity_type, o.entity_id,
           o.reading, o.omen, o.sigil_score, o.generated_at,
           o.model_version, o.prompt_version
    FROM public.oracle_readings o
    JOIN public.sports sp ON sp.id = o.sport
    WHERE o.reading IS NOT NULL AND o.season = sp.current_season
    ORDER BY o.sport, o.entity_type, o.entity_id, o.generated_at DESC
)
UPDATE public.sigil_synthesis ss
SET reading              = lr.reading,
    omen                 = lr.omen,
    voiced_score         = lr.sigil_score,
    voiced_at            = lr.generated_at,
    voice_model_version  = lr.model_version,
    voice_prompt_version = lr.prompt_version
FROM live_latest ll
JOIN live_reading lr
  ON (lr.sport, lr.entity_type, lr.entity_id) = (ll.sport, ll.entity_type, ll.entity_id)
WHERE ss.id = ll.id AND ss.reading IS NULL;

-- HISTORICAL explicit-season copy (same rule per non-current season).
WITH hist_latest AS (
    SELECT DISTINCT ON (ss.sport, ss.entity_type, ss.entity_id, ss.season)
           ss.id, ss.sport, ss.entity_type, ss.entity_id, ss.season
    FROM public.sigil_synthesis ss
    JOIN public.sports sp ON sp.id = ss.sport
    WHERE ss.score IS NOT NULL
      AND ss.season IS NOT NULL AND ss.season <> sp.current_season
    ORDER BY ss.sport, ss.entity_type, ss.entity_id, ss.season, ss.generated_at DESC
),
hist_reading AS (
    SELECT DISTINCT ON (o.sport, o.entity_type, o.entity_id, o.season)
           o.sport, o.entity_type, o.entity_id, o.season,
           o.reading, o.omen, o.sigil_score, o.generated_at,
           o.model_version, o.prompt_version
    FROM public.oracle_readings o
    JOIN public.sports sp ON sp.id = o.sport
    WHERE o.reading IS NOT NULL AND o.season <> sp.current_season
    ORDER BY o.sport, o.entity_type, o.entity_id, o.season, o.generated_at DESC
)
UPDATE public.sigil_synthesis ss
SET reading              = hr.reading,
    omen                 = hr.omen,
    voiced_score         = hr.sigil_score,
    voiced_at            = hr.generated_at,
    voice_model_version  = hr.model_version,
    voice_prompt_version = hr.prompt_version
FROM hist_latest hl
JOIN hist_reading hr
  ON (hr.sport, hr.entity_type, hr.entity_id, hr.season)
   = (hl.sport, hl.entity_type, hl.entity_id, hl.season)
WHERE ss.id = hl.id AND ss.reading IS NULL;

-- The freeze. Nothing writes this table after the Session C release (the Rust
-- dual-write stops in the same commit that flips Go serving to the merged
-- columns); nothing serves from it. Kept as history; drop in a later season.
COMMENT ON TABLE public.oracle_readings IS
    'FROZEN read-only history (Session C, 2026-07-16). The Oracle voice lives on '
    'sigil_synthesis (reading/omen/voiced_* — mig 152) since the sigil-voice merge; '
    'the latest real reading per serving scope was copied there by mig 153. '
    'No reader, no writer. Retained for provenance; candidate for DROP in a later season.';

INSERT INTO public.schema_migrations(version) VALUES ('153_sigil_voice_copy_freeze')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

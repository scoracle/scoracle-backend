-- 217_storyline_attach_provenance.sql
--
-- WHAT: four nullable observation columns on `storyline_articles` — `attach_score`,
-- `matched_entities`, `seed_size`, `candidate_count`. Nothing is read; nothing changes shape.
--
-- WHY: 6.7's band verdict instructs the reader to "inspect attach scores", and there are none.
-- `storyline_articles` is four columns — (storyline_id, article_id, attached_at, attach_method) —
-- with `attach_method='auto'` on every live row, so it discriminates nothing. A wrong merge
-- therefore cannot be audited after the fact, only re-derived by re-running the code. The score is
-- ALREADY computed at attach time (`editor/storyline.rs` `pick`) and thrown away at the INSERT;
-- this records it. Same lesson as D-T30 and the archbox mirror: we keep being asked to read an
-- observation we never recorded.
--
-- This is INSTRUMENTATION, NOT A FIX. The attachment rule — PERSON_OVERLAP_POINTS,
-- TEAM_OVERLAP_POINTS, ATTACH_THRESHOLD, covers_seed — is untouched by this migration and by the
-- commit that carries it. 6.7's candidate fixes are unmeasured and stay unmeasured until these
-- columns have live rows to measure. ⛔ Do NOT change a threshold on this evidence: p50=1, p90=3,
-- and a global change would break 3,890 healthy storylines to repair three.
--
-- All four are NULLABLE with no backfill, deliberately. The 19,920 existing rows (7,349 auto /
-- 12,571 backfill) predate the instrumentation and NULL is the honest value for them — a
-- reconstructed score would be a theory wearing a measurement's clothes. Read `attach_score IS
-- NULL` as "recorded before 217", never as "scored zero".
--
-- Deploy order: ADDITIVE, and inert until the cognition binary that writes them ships. Apply
-- BEFORE that deploy (db.New prepares statements at boot and fail-fasts on a drifted schema);
-- applying it early costs nothing because every column is nullable and unread.

BEGIN;

ALTER TABLE public.storyline_articles
    ADD COLUMN IF NOT EXISTS attach_score     int,
    ADD COLUMN IF NOT EXISTS matched_entities text[],
    ADD COLUMN IF NOT EXISTS seed_size        int,
    ADD COLUMN IF NOT EXISTS candidate_count  int;

COMMENT ON COLUMN public.storyline_articles.attach_score IS
    'The winning score from editor/storyline.rs `pick` (weighted entity overlap + 1 story_type '
    'match + 1 recency), as measured at attach time. NULL when this article OPENED the storyline '
    '(no candidate won, so there is no score to record) and NULL on every row written before '
    'migration 217. Never read NULL as zero.';

COMMENT ON COLUMN public.storyline_articles.matched_entities IS
    'Which of the storyline''s SEED participants this article actually shared, as '
    '`entity_type:entity_id`. THE column 6.7 needed and could not query: it names WHY the merge '
    'happened. Note the join key is the FROZEN SEED (storyline_entities at min(joined_at)), not '
    'the storyline''s full participant set — so a storyline carrying 169 entities still matches on '
    'its original 3-5. NULL on an opening article and on pre-217 rows.';

COMMENT ON COLUMN public.storyline_articles.seed_size IS
    'The winning storyline''s seed cast size at attach time — the denominator of covers_seed '
    '(shared * 2 >= seed_size). Recorded because the gate is a RATIO: a seed of 4 admits any '
    'article sharing 2, and when those 4 include two hot clubs that is a large slice of a day''s '
    'transfer stream. NULL on an opening article and on pre-217 rows.';

COMMENT ON COLUMN public.storyline_articles.candidate_count IS
    'How many open storylines were scored for this article. Written on EVERY row, including '
    'openings — on an opening it reads "N candidates existed and none cleared the bar", which is '
    'the observation that separates "no candidates" from "candidates rejected". NULL only on '
    'pre-217 rows.';

INSERT INTO public.schema_migrations(version) VALUES ('217_storyline_attach_provenance')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

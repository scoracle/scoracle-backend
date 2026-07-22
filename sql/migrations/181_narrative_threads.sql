-- 181_narrative_threads.sql
--
-- Characters Phase C (progressing-narrative threads): entity-keyed STORYLINE IDENTITY.
-- Until now a storyline's identity was its title string (persist_narratives matched
-- DISTINCT ON narrative_title vs prior rows) — the F5 seam: any title drift reset the
-- trajectory to developing_story, proven live by the 07-22 n10 wave resetting ALL
-- trajectories. This migration gives storylines a durable identity: `narrative_threads`
-- rows keyed by entity, matched by BGE-small embedding cosine (>= 0.80) against a
-- recency-weighted centroid, progressed by every attached news_summaries row.
--
-- The Rust side (same branch tip) replaces the title-match seam with the thread attach:
-- embed title+body -> cosine vs the entity's open-thread centroids -> attach (progress)
-- or open a new thread; classify_delta runs vs the THREAD's last_impact, so
-- heating_up/cooling_off survive title drift. Sealing is a nightly sweep
-- (seal_narrative_threads, added to cron-narrative-links.sh): ground-truth confirmed
-- move -> 'resolved' (checked FIRST, mirroring mig 157's confirmation-beats-quiet-seal),
-- quiet >= 21d -> 'faded'.
--
-- ADDITIVE and binary-compatible: the running binary ignores the new table/column; the
-- new binary requires this migration (deploy order: migrate, then restart — db.New
-- fail-fast is the Go-side guard, the Rust daemon reads these tables lazily).
-- Trajectory vocabulary is UNCHANGED (trajectory.rs / Go CASE labels / mig 154 SQL
-- buckets — the three homes stay in sync; only the comparison anchor moves).

BEGIN;

CREATE TABLE IF NOT EXISTS public.narrative_threads (
    id                 bigserial PRIMARY KEY,
    sport              text NOT NULL REFERENCES sports(id),
    entity_type        text NOT NULL CHECK (entity_type IN ('player', 'team')),
    entity_id          integer NOT NULL,
    -- The story's current name: updated to the LATEST attached telling's title, so the
    -- thread is always named by its freshest chapter (titles drift; identity does not).
    canonical_title    text NOT NULL,
    status             text NOT NULL DEFAULT 'open' CHECK (status IN ('open', 'sealed')),
    -- Sealed threads carry HOW they ended: 'resolved' (transfer ground truth confirmed
    -- the story) or 'faded' (quiet >= 21 days). Open threads have no outcome.
    outcome            text CHECK (outcome IN ('resolved', 'faded')),
    opened_at          timestamptz NOT NULL DEFAULT now(),
    last_progressed_at timestamptz NOT NULL DEFAULT now(),
    sealed_at          timestamptz,
    entry_count        integer NOT NULL DEFAULT 0,
    peak_impact        smallint CHECK (peak_impact IS NULL OR (peak_impact >= 0 AND peak_impact <= 100)),
    last_impact        smallint CHECK (last_impact IS NULL OR (last_impact >= 0 AND last_impact <= 100)),
    last_trajectory    text NOT NULL DEFAULT 'developing_story'
                       CHECK (last_trajectory IN ('developing_story', 'heating_up', 'cooling_off')),
    distinct_sources   integer NOT NULL DEFAULT 0,
    source_names       text[] NOT NULL DEFAULT '{}'::text[],
    -- L2-normalized BGE-small centroid (384 dims), recency-weighted (EWMA in Rust) so the
    -- centroid follows the story as it evolves. real[] — matches the f32 the embedder emits.
    centroid           real[],
    CONSTRAINT narrative_threads_sealed_shape CHECK (
        (status = 'open'   AND outcome IS NULL     AND sealed_at IS NULL) OR
        (status = 'sealed' AND outcome IS NOT NULL AND sealed_at IS NOT NULL)
    )
);

COMMENT ON TABLE public.narrative_threads IS
    'Entity-keyed storyline identity (Characters Phase C, mig 181). One row per progressing '
    'story; news_summaries rows attach via thread_id by embedding-cosine match (>= 0.80 vs '
    'centroid, BGE-small, EWMA-updated). Replaces title-string storyline identity (F5): '
    'trajectory is classified vs the thread''s last_impact, so it survives title drift. '
    'Sealed nightly by seal_narrative_threads(): ground truth -> resolved, quiet 21d -> faded.';

-- The hot lookup: the attach path loads an entity's OPEN threads every generation.
CREATE INDEX IF NOT EXISTS idx_narrative_threads_open_entity
    ON public.narrative_threads (sport, entity_type, entity_id)
    WHERE status = 'open';

-- The sweep + "story so far" surfaces scan by recency.
CREATE INDEX IF NOT EXISTS idx_narrative_threads_progressed
    ON public.narrative_threads (sport, last_progressed_at DESC);

ALTER TABLE public.news_summaries
    ADD COLUMN IF NOT EXISTS thread_id bigint REFERENCES public.narrative_threads(id) ON DELETE SET NULL;

COMMENT ON COLUMN public.news_summaries.thread_id IS
    'The progressing storyline this row is a chapter of (mig 181). NULL for marker rows, '
    'pre-thread history the backfill has not visited, and rows persisted without an embedder.';

-- Per-thread chapter walk (newest-first) for the card + v_narrative_threads.
CREATE INDEX IF NOT EXISTS idx_news_summaries_thread
    ON public.news_summaries (thread_id, generated_at DESC)
    WHERE thread_id IS NOT NULL;

-- ---------------------------------------------------------------------------
-- Nightly seal sweep — runs from cron-narrative-links.sh after the episode roll.
-- Order inside matters: ground-truth resolution is checked FIRST so a confirmed move
-- beats a same-night quiet-fade (the mig-157 lesson, applied to threads).
-- ---------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.seal_narrative_threads(p_sport text)
RETURNS TABLE (resolved_count integer, faded_count integer)
LANGUAGE sql
AS $$
WITH transfer_flavored AS (
    -- A thread is transfer-flavored when any attached chapter cites an article the
    -- Journalist bucketed 'transfer' (n9 buckets). Only these can resolve via ground
    -- truth — a player's injury saga must not seal because an unrelated move confirmed.
    SELECT DISTINCT t.id
    FROM narrative_threads t
    JOIN news_summaries s ON s.thread_id = t.id
    JOIN news_articles na ON na.id = ANY (s.input_news_ids) AND na.bucket = 'transfer'
    WHERE t.sport = p_sport AND t.status = 'open'
),
resolved AS (
    UPDATE narrative_threads t
       SET status = 'sealed', outcome = 'resolved', sealed_at = g.applied_at
      FROM (
          SELECT tf.id, min(g.applied_at) AS applied_at
          FROM transfer_flavored tf
          JOIN narrative_threads t2 ON t2.id = tf.id
          JOIN transfer_ground_truth g ON g.sport = t2.sport
           AND ((t2.entity_type = 'player' AND g.player_id = t2.entity_id)
             OR (t2.entity_type = 'team'   AND g.team_id   = t2.entity_id))
           AND g.applied_at >= t2.opened_at
          GROUP BY tf.id
      ) g
     WHERE t.id = g.id AND t.status = 'open'
    RETURNING t.id
),
faded AS (
    UPDATE narrative_threads t
       SET status = 'sealed', outcome = 'faded',
           -- Deterministic: the moment the quiet rule tripped, not when the sweep ran.
           sealed_at = t.last_progressed_at + interval '21 days'
     WHERE t.sport = p_sport AND t.status = 'open'
       AND t.last_progressed_at < now() - interval '21 days'
       AND t.id NOT IN (SELECT id FROM resolved)
    RETURNING t.id
)
SELECT (SELECT count(*) FROM resolved)::integer,
       (SELECT count(*) FROM faded)::integer;
$$;

COMMENT ON FUNCTION public.seal_narrative_threads(text) IS
    'Nightly thread seal sweep (mig 181, cron-narrative-links.sh): transfer-flavored open '
    'threads with a ground-truth move applied since opening seal as resolved (checked first); '
    'open threads quiet >= 21 days seal as faded (sealed_at = last_progressed_at + 21d).';

INSERT INTO public.schema_migrations(version) VALUES ('181_narrative_threads')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

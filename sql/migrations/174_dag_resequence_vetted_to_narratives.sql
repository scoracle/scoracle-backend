-- 174_dag_resequence_vetted_to_narratives.sql
--
-- Cognition refactor, Phase 3 (DAG re-sequence). The scrub `vetted` trigger now gates ONLY into
-- narratives (+ the per-article graph). The old parallel fan-out to transfers and vibe is removed:
--   - transfers: re-pointed to mig 175's `news_articles.bucket → 'transfer'` trigger — the
--     Journalist (n9) labels each article, and only genuinely transfer-related articles route to
--     the transfers stage, instead of every vetted team firing transfers every cycle.
--   - vibe: re-pointed downstream of narratives — the narratives handler enqueues vibe after it
--     persists (Rust, mirroring the vibe → momentum hand-off). Vibe reads the narrative + transfer
--     reads, so it belongs after them, not parallel to them.
--
-- Also: the narratives corpus membership count/fingerprint (the mig-103 freshness gate + reopen
-- key) now filters `a.duplicate_of IS NULL`, matching the widened narratives corpus loader
-- (canonical only). A repost the scrub novelty gate suppressed no longer perturbs the fingerprint
-- or re-fires narratives.
--
-- SHIPS TOGETHER with the n9 binary (narratives writes news_articles.bucket + enqueues vibe) and
-- mig 175 — the transfers/vibe hand-offs move out of this trigger and into the binary + mig 175 in
-- the same cutover. Deploy order (F-022): the binary is forward/backward compatible with both the
-- old and new trigger (it enqueues vibe itself and tolerates the old trigger also enqueuing it —
-- `work.enqueue` is idempotent), so ship the binary FIRST, then apply mig 174 + 175 together, then
-- run the one-time narratives regen backfill (a plain enqueue is NOT enough — the handler debounces
-- on the corpus input_hash, which is unchanged; force it by NULLing the latest news_summaries
-- input_hash per entity, or bump the narratives input-hash pre-image). Then snapshot-schema.

BEGIN;

CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_count       integer;
    v_fp          text;
    v_ver         text;
    v_graph_count integer;
BEGIN
    -- Per-ARTICLE graph extraction (mig 165): one item per article, reopened when the article's
    -- vetted membership changes; the handler debounces on material hash, so reopens are cheap.
    -- BEFORE the freshness gate below — extraction of a backfilled old article is durable-memory
    -- work the 72h corpus gate must not suppress. UNCHANGED by Phase 3.
    SELECT count(*) INTO v_graph_count
      FROM public.news_article_entities
     WHERE article_id = NEW.article_id AND vetted IS TRUE;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('graph', 'article', NEW.article_id::integer, NEW.sport, 'pending',
            'g:' || v_graph_count, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Canonical membership fingerprint over this entity's vetted, in-lookback (72h) links. Phase 3:
    -- `a.duplicate_of IS NULL` keeps only originals — a suppressed repost never perturbs the
    -- fingerprint or re-fires narratives, matching the narratives corpus loader. Doubles as the
    -- mig-103 freshness gate: a stale backlog primary with no fresh vetted canonical corpus yields
    -- count=0 and enqueues nothing. One link row per (article, entity) by PK; ORDER BY makes the
    -- aggregate deterministic.
    SELECT count(*),
           md5(string_agg(nae.article_id::text, ',' ORDER BY nae.article_id))
      INTO v_count, v_fp
      FROM public.news_article_entities nae
      JOIN public.news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND a.duplicate_of IS NULL
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_fp;

    -- Phase 3: gate ONLY into narratives. transfers ← mig-175 bucket trigger; vibe ← narratives
    -- handler (Rust). The team-vs-player fan-out and the vibe/transfers stages are gone from here.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('narratives', NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Redundant with the mig-133 statement-level pipeline_work notify trigger, but kept for the
    -- minimal diff: pg_notify de-dups identical (channel,payload) within a txn, so this costs
    -- nothing extra.
    PERFORM pg_notify('pipeline_work_ready', '');

    RETURN NEW;
END;
$$;

INSERT INTO public.schema_migrations(version) VALUES ('174_dag_resequence_vetted_to_narratives')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

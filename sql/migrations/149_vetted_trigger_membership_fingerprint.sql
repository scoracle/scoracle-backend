-- 149_vetted_trigger_membership_fingerprint.sql
--
-- F4 of the flow-friction plan (2026-07-12): fingerprint the vetted-set MEMBERSHIP in the
-- vetted-trigger's input_version. LANDMARK (queue semantics) — flat wiki progress doc at
-- scoracle-wiki/progress_docs/2026-07-12_vetted-trigger-membership-fingerprint.md.
--
-- Problem. Since mig 103 the input_version has been `count:max(scrubbed_at)-epoch` over the
-- entity's 72h vetted corpus. But scrubbed_at is a touch timestamp, not a membership fact:
-- scrub's apply_verdicts (rust/src/scrub.rs) re-stamps EVERY link of the article it processes —
-- including TRUE→TRUE re-vets, which do NOT fire this trigger — so max(scrubbed_at) advances
-- silently between firings. The next membership-neutral firing (e.g. a backlog article published
-- >72h ago vets, leaving the in-window set unchanged) then computes a DISTINCT version, and the
-- ON CONFLICT guard reopens narratives/vibe/transfers work on zero material change. This is the
-- reopen-pressure amplifier behind whole-team transfer re-vets (F3's per-pair skip caps what each
-- reopen costs on the GPU; F4 removes the reopen itself).
--
-- Change. input_version = count ':' md5(comma-joined sorted in-window vetted article_ids).
-- The version now moves iff the 72h vetted membership actually changes. A re-vet of the same set
-- no longer reopens pending work — the DESIRED under-triggering: same evidence, no new
-- derivation. (Aging OUT of the 72h window alone still doesn't fire the trigger — unchanged from
-- the epoch formula; the source-freshness cooling-off doctrine owns staleness.)
--
-- Recipe ownership (verified 2026-07-12): this trigger is the SOLE enqueuer of
-- narratives/vibe/transfers and the sole producer of their input_version — Go's CorpusVersion is
-- retired (corpus.go exports only Sweep/LoadTeams), vibesynth/listener enqueue only sigil,
-- maintenance enqueues only scrub, and nothing parses `count:epoch` back out of pipeline_work.
-- Function lineage: 103 → 113 → 121 → 149 (this file); derived from the mig-147 schema snapshot
-- (sql/schema/schema.sql @ backend 75f356d), which is byte-identical to mig 121's definition —
-- pre-flight `\sf public.enqueue_derive_on_vetted` at apply time per README-migrations step 3.
-- The trigger definition itself, the WHEN guard, and idx_nae_vetted_lookup are untouched.
--
-- Format note: old `count:epoch` values can never equal new `count:md5hex` values, so every
-- entity's FIRST post-149 firing reopens its derive work once — a bounded one-time wave (same
-- class as the F1/F3 hash-composition waves), then steady state.
--
-- Deploy order: no binary dependency, but apply WITH or AFTER the F3 cognition deploy so the
-- 149 reopen wave and F3's NULL-input_hash regeneration wave overlap instead of paying twice
-- (the F1/F2/F3 material gates make each reopened drain skip unchanged work before the model).

BEGIN;

CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS TRIGGER AS $$
DECLARE
    v_count  integer;
    v_fp     text;
    v_ver    text;
    v_stages text[] := ARRAY['narratives', 'vibe'];
BEGIN
    -- Membership fingerprint over this entity's vetted, in-lookback (72h) links. Doubles as the
    -- mig-103 freshness gate: a stale backlog primary with no fresh vetted corpus yields count=0
    -- and enqueues nothing. One link row per (article, entity) by PK, so no DISTINCT needed;
    -- ORDER BY makes the aggregate deterministic.
    SELECT count(*),
           md5(string_agg(nae.article_id::text, ',' ORDER BY nae.article_id))
      INTO v_count, v_fp
      FROM public.news_article_entities nae
      JOIN public.news_articles a ON a.id = nae.article_id
     WHERE nae.entity_type = NEW.entity_type
       AND nae.entity_id   = NEW.entity_id
       AND nae.sport       = NEW.sport
       AND nae.vetted IS TRUE
       AND (a.published_at IS NULL OR a.published_at > NOW() - INTERVAL '72 hours');

    IF v_count = 0 THEN
        RETURN NEW;
    END IF;

    v_ver := v_count || ':' || v_fp;

    IF NEW.entity_type = 'team' THEN
        v_stages := ARRAY['transfers', 'narratives', 'vibe'];
    END IF;

    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT st, NEW.entity_type, NEW.entity_id, NEW.sport, 'pending', v_ver, NOW(), NOW()
      FROM unnest(v_stages) AS st
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
$$ LANGUAGE plpgsql;

INSERT INTO public.schema_migrations(version) VALUES ('149_vetted_trigger_membership_fingerprint')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.
-- Post-apply check (plan §F4): re-touch an already-vetted entity's corpus (a TRUE→TRUE re-scrub)
-- and confirm no pending pipeline_work row for it flips available_at/input_version.

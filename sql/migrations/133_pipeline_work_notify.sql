-- 133_pipeline_work_notify.sql
--
-- A8 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): make `pipeline_work_ready` a NOTIFY
-- property of the pipeline_work TABLE, not a discipline of every writer.
--
-- Before this migration the only producer of the `pipeline_work_ready` channel was
-- enqueue_derive_on_vetted() (the mig-103 → 113 → 121 vetted-trigger lineage). Three
-- Go callers enqueue into pipeline_work WITHOUT notifying — listener.go (composite
-- percentile shift → StageSigil), cmd/vibesynth (nightly reconcile → StageSigil), and
-- maintenance.go (candidate-rich articles → StageScrub) — so the Rust daemon only
-- picked those rows up on its 30s safety-net tick. A table trigger fixes that for
-- every writer, present and future (including any direct-INSERT path), permanently.
--
-- DECIDED (plan A8, option b): AFTER INSERT OR UPDATE trigger on the table.
--
-- Notes on the NOTIFY volume (all within the accepted envelope):
--   * The trigger also fires on the Rust daemon's OWN enqueues (e.g. vibe → sigil at
--     vibe.rs:660) — harmless self-wakeups: the daemon wakes, drains, finds its own
--     freshly-committed row, done.
--   * pg_notify de-dups identical (channel,payload) only WITHIN a transaction, so
--     vibesynth's row-per-txn reconcile loop produces a burst of one cheap empty
--     NOTIFY per committed row. Each is a bounded wake-drain-sleep on the daemon.
--   * FOR EACH STATEMENT (not FOR EACH ROW): the enqueue function inserts several
--     stage rows in one statement; a statement-level trigger fires the NOTIFY once
--     instead of once-per-row, and the per-txn de-dup makes the delivered
--     notification identical either way. Trade-off: a zero-row UPDATE (e.g. an
--     ON CONFLICT ... WHERE that matches nothing) still fires one empty tick — a
--     harmless self-wakeup, same class as the above.

BEGIN;

CREATE OR REPLACE FUNCTION public.notify_pipeline_work_ready() RETURNS trigger AS $$
BEGIN
    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NULL;  -- AFTER STATEMENT trigger: return value is ignored.
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS pipeline_work_notify ON public.pipeline_work;
CREATE TRIGGER pipeline_work_notify
    AFTER INSERT OR UPDATE ON public.pipeline_work
    FOR EACH STATEMENT
    EXECUTE FUNCTION public.notify_pipeline_work_ready();

INSERT INTO public.schema_migrations(version) VALUES ('133_pipeline_work_notify')
    ON CONFLICT DO NOTHING;

COMMIT;

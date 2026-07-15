-- 150_pipeline_work_notify_pending_only.sql
--
-- Incident follow-up (2026-07-15 NOTIFY-queue jam, follow-up #4): stop notifying on
-- statements that make no work ready. The mig-133 statement-level trigger fires
-- `pipeline_work_ready` on EVERY INSERT/UPDATE statement — including the daemon's own
-- claim UPDATEs (pending → running) and, critically, 0-row claim attempts. During the
-- incident the daemon's 5-second safety-net claims fed the jammed queue its own
-- listener would never read; in steady state every claim is a pointless self-wakeup.
--
-- Change: notify only when the statement's transition table contains at least one row
-- that is actually claimable (`status='pending' AND available_at <= NOW()`):
--   * enqueue INSERT / conflict-reopen ..................... pending, now  → notify
--   * requeue_stale / release (shutdown handback) .......... pending, now  → notify
--   * claim (pending → running), incl. 0-row statements .... no pending    → SILENT
--   * fail (→ 'failed' with a future available_at) ......... not claimable → SILENT
--     (retries ride the safety-net tick once backoff elapses — backoff ≥ 30 min
--     dwarfs the 5-30s safety net, so retry latency is unchanged in practice)
--
-- Postgres does not allow transition tables on multi-event triggers, and a statement
-- trigger's WHEN clause cannot reference them either — hence two single-event
-- triggers sharing one function that inspects the `new_rows` transition table.
--
-- Defense in depth with the 2026-07-15 worker split (drain/supervisor tasks): the
-- Rust supervisor now always reads the LISTEN socket, so an unread-socket queue jam
-- is structurally gone; this migration shrinks the NOTIFY volume itself, protecting
-- any future listener as well.
--
-- NOT applied at author time — production DB migration requires named-user approval.
-- Sequencing: independent of the worker-split binaries; either side can land first.

BEGIN;

CREATE OR REPLACE FUNCTION public.notify_pipeline_work_ready() RETURNS trigger AS $$
BEGIN
    -- `new_rows` is the statement's transition table (REFERENCING NEW TABLE below).
    IF EXISTS (
        SELECT 1 FROM new_rows
        WHERE status = 'pending' AND available_at <= NOW()
    ) THEN
        PERFORM pg_notify('pipeline_work_ready', '');
    END IF;
    RETURN NULL;  -- AFTER STATEMENT trigger: return value is ignored.
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS pipeline_work_notify ON public.pipeline_work;

CREATE TRIGGER pipeline_work_notify_insert
    AFTER INSERT ON public.pipeline_work
    REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION public.notify_pipeline_work_ready();

CREATE TRIGGER pipeline_work_notify_update
    AFTER UPDATE ON public.pipeline_work
    REFERENCING NEW TABLE AS new_rows
    FOR EACH STATEMENT
    EXECUTE FUNCTION public.notify_pipeline_work_ready();

INSERT INTO public.schema_migrations(version) VALUES ('150_pipeline_work_notify_pending_only')
    ON CONFLICT DO NOTHING;

COMMIT;

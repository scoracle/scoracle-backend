-- 256_pipeline_work_claim_fencing.sql
--
-- A pipeline_work key can be reopened for newer input or recovered after its lease expires while
-- the original worker is still alive. The old lifecycle guarded complete/fail/defer/release only
-- with status='running', so that original worker could mutate a later claimant's running row.
-- Give every running lease a database-issued identity and preserve the exact desired revision
-- captured by that lease. Queue acknowledgements can now require key + token + running revision.
--
-- This is deliberately only the queue-ownership fence. Product publication still needs to join
-- ownership validation, the product/required provenance write, follow-up intent, and completion in
-- a short transaction; no transaction is held across inference.
--
-- Deploy order: apply this additive schema first, then drain/stop every status-only worker before
-- starting a token-aware worker. The trigger keeps older statements schema-compatible, but an old
-- worker cannot participate in the fence because its acknowledgement does not present a token.

BEGIN;

ALTER TABLE public.pipeline_work
    ADD COLUMN IF NOT EXISTS running_input_version text,
    ADD COLUMN IF NOT EXISTS claim_token uuid;

-- Claims that predate this migration remain operable during a rolling deploy and become visibly
-- owned. A restarted token-aware worker will only receive tokens returned by its own later claim.
UPDATE public.pipeline_work
   SET running_input_version = input_version,
       claim_token = gen_random_uuid()
 WHERE status = 'running'
   AND claim_token IS NULL;

CREATE OR REPLACE FUNCTION public.normalize_pipeline_work_claim()
RETURNS trigger
LANGUAGE plpgsql
AS $$
BEGIN
    IF NEW.status = 'running' THEN
        -- Mint on every transition into running. Preserve the token on unrelated updates to the
        -- same live lease; a claimant must not lose ownership because observability was updated.
        IF TG_OP = 'INSERT' THEN
            NEW.claim_token := gen_random_uuid();
            NEW.running_input_version := NEW.input_version;
        ELSIF OLD.status IS DISTINCT FROM 'running'
              OR NEW.claim_token IS NULL THEN
            NEW.claim_token := gen_random_uuid();
            NEW.running_input_version := NEW.input_version;
        END IF;
    ELSE
        -- Pending/failed rows describe desired work, not an active execution.
        NEW.claim_token := NULL;
        NEW.running_input_version := NULL;
    END IF;
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS pipeline_work_claim_identity ON public.pipeline_work;
CREATE TRIGGER pipeline_work_claim_identity
    BEFORE INSERT OR UPDATE OF status, input_version, claim_token
    ON public.pipeline_work
    FOR EACH ROW
    EXECUTE FUNCTION public.normalize_pipeline_work_claim();

ALTER TABLE public.pipeline_work
    DROP CONSTRAINT IF EXISTS pipeline_work_running_claim_check;
ALTER TABLE public.pipeline_work
    ADD CONSTRAINT pipeline_work_running_claim_check
    CHECK ((status = 'running') = (claim_token IS NOT NULL));

COMMENT ON COLUMN public.pipeline_work.input_version IS
    'Latest desired input revision. A changed value reopens work even while an older revision runs.';
COMMENT ON COLUMN public.pipeline_work.running_input_version IS
    'Desired input revision captured by the active claim; NULL when no claim is running.';
COMMENT ON COLUMN public.pipeline_work.claim_token IS
    'Unique active lease identity. Claim-sensitive mutations must match this token and running revision.';
COMMENT ON TABLE public.pipeline_work IS
    'Outstanding per-entity derivation work. input_version is desired input; running_input_version '
    'and claim_token identify the active fenced lease. Completed work is deleted.';

INSERT INTO public.schema_migrations(version) VALUES ('256_pipeline_work_claim_fencing')
    ON CONFLICT DO NOTHING;

COMMIT;

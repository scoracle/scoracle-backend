-- 228_stat_summaries_trigger_type_widening.sql
--
-- LATENT DEFECT — armed since 615bdcb ("the Scout learns when a transfer became concrete",
-- Scott's 2026-08-15 brief), never yet fired. Ships AHEAD of the availability work that found
-- it, because that work is what would finally fire it.
--
-- ## What is broken
--
-- `RatingHandler::handle` writes `trigger_type = "transfer"` for any rating opened by an
-- adjudicated move, and `persist_stat_summary` binds that string straight through to the INSERT
-- with no mapping. But `stat_summaries.trigger_type` has carried
-- `CHECK (trigger_type IN ('stat_change','periodic','manual'))` since mig 086 and **no migration
-- has ever widened it**. `'transfer'` is not in the list.
--
-- So the FIRST transfer-triggered Scout run will do all of the expensive work and then throw:
--
--   1. the item is claimed on the contended archbox card;
--   2. `skip_unchanged` is deliberately DISABLED for these items, so the model call always runs;
--   3. the card is generated;
--   4. the INSERT violates `stat_summaries_trigger_type_check` and the row is lost;
--   5. the work item fails, retries, and burns the card again.
--
-- ## Measured on prod 2026-08-23 — it has cost nothing YET, and here is why
--
--   * the live constraint is confirmed narrow (`pg_get_constraintdef`: stat_change, periodic,
--     manual);
--   * `stat_summaries` holds 20,322 `periodic` + 163 `manual` rows and **zero** `transfer`;
--   * `pipeline_work` has **never** held a rating row carrying the `xfer` marker;
--   * no `last_error` on any stage mentions `trigger_type`;
--   * because all five all-time applied transfers landed in JULY (last: 2026-07-29), BEFORE the
--     trigger existed, and nothing has reached `applied` since. The trigger Scott commissioned
--     on 08-15 has had zero opportunities to run.
--
-- The defect is therefore real, unexercised, and armed. It bites on the next applied transfer
-- or — sooner — on the first applied availability event, which is why it is fixed here first.
--
-- ## Why widening is the fix, rather than mapping 'transfer' onto 'stat_change'
--
-- The column answers "what kicked off this generation" (mig 086's own comment), and a transfer
-- is not a stat change — that is the entire point of the trigger. Folding it into an existing
-- value would erase the provenance the eval split needs to tell an availability-triggered card
-- from a periodic one, which is the same reason `'availability'` is added here rather than
-- later: mig 229's record and the Scout's availability enqueue write it, and shipping that
-- feature onto the un-widened constraint would reproduce this bug immediately.
--
-- ## Safety
--
-- No existing row can violate the new constraint — it is a strict superset of the old one. And
-- no `'transfer'` row exists to validate, because the constraint being fixed is what rejected
-- them all. Validation is therefore instant; there is no long ACCESS EXCLUSIVE hold.

BEGIN;

ALTER TABLE public.stat_summaries
    DROP CONSTRAINT IF EXISTS stat_summaries_trigger_type_check;

ALTER TABLE public.stat_summaries
    ADD CONSTRAINT stat_summaries_trigger_type_check
    CHECK (trigger_type = ANY (ARRAY[
        'stat_change'::text,   -- the percentile_changed listener (mig 086)
        'periodic'::text,      -- the nightly statcommentary batch; the overwhelming majority
        'manual'::text,        -- operator-run generations
        'transfer'::text,      -- an adjudicated move crossed the concrete threshold (615bdcb)
        'availability'::text   -- an injury or suspension was applied (mig 229)
    ]));

COMMENT ON COLUMN public.stat_summaries.trigger_type IS
    'What kicked off this generation. ''stat_change'' = the percentile_changed listener; '
    '''periodic'' = the nightly batch; ''manual'' = operator-run; ''transfer'' = an adjudicated '
    'move crossed the concrete threshold; ''availability'' = an injury or suspension was applied '
    '(mig 229). The last two are the non-statistical triggers — they run with the '
    'skip_unchanged debounce disabled, because the input_hash deliberately does not move for '
    'them, and this column is the only record of which one woke the seat.';

INSERT INTO public.schema_migrations(version) VALUES ('228_stat_summaries_trigger_type_widening')
    ON CONFLICT DO NOTHING;

COMMIT;

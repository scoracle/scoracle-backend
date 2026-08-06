-- PLAN-one-rail 8.6 — THE FLIP, the SQL half. §2's "one act", in one transaction.
--
-- ############################################################################
-- ##  DO NOT APPLY until the flip is happening. Applying this WITHOUT the    ##
-- ##  RAIL=packet deploy in the same sitting stops the legacy rail dead      ##
-- ##  while nothing has taken over: no article_read enqueue, no transfers    ##
-- ##  enqueue, and voices waiting on packet work that legacy never compiles. ##
-- ############################################################################
--
-- This file replaces 8.3's prepared migration AND the held 7.4 seed, because the plan requires
-- them to land together (§2: "the flip is one act"). Apply it as migration 213 on flip day:
--
--     cp sql/prepared/8.6_flip_day.sql sql/migrations/213_retire_legacy_rail.sql
--     ./sql/migrate.sh          # it self-records inside its own transaction
--
-- ## The order of the flip (this file is STEP 1)
--
--   1. THIS SQL           — triggers dropped, subscriptions seeded, atomically
--   2. [DEPLOY] Go        — RAIL=packet in .env.local; persistArticles stops enqueueing
--                           `scrub` and stops guessing regex secondary links (8.4)
--   3. [DEPLOY] Rust      — same RAIL=packet; COGNITION_STAGES drops `article_read` + `scrub`
--   4. scripts/rail-cutover-check.sh against the live flip; snapshot-schema; commit
--
-- Triggers BEFORE the Editor ever writes `vetted` (T10) is the whole reason step 1 leads: the
-- Editor's 8.5 link write sets vetted=TRUE, and `enqueue_derive_on_vetted` turns exactly that
-- into an `article_read` work row. Flip the rail first and every Editor read would enqueue work
-- for a stage no worker claims any more.
--
-- ## What is dropped, and why each one
--
--   * `enqueue_derive_on_vetted` (news_article_entities) — enqueues `article_read`, the stage
--     this act deletes. Its replacement is Go enqueueing `editor` directly at ingest, which has
--     been live since 3.5. T10 dies with this trigger.
--   * `enqueue_transfers_if_transfer_related` (news_articles, mig 175) — the article-grain
--     transfers enqueue writing `t:<count>:<md5>`. Its replacement is mig 206 arm 1 via the
--     ('transfer','transfers','team') subscription seeded below. Leaving both is the mig-197
--     churn loop D-T15 measured: two writers, two prefixes, one pipeline_work row.
--   * `enqueue_voices_on_routing_tags` (news_articles, mig 197) — **DEVIATION FROM 8.3, and it
--     is deliberate; raise it if you disagree.** 8.3 says leave this one for Phase 9, reasoning
--     that post-flip it fires into no article-grain rows because nothing writes `routing_tags`
--     any more. That is true post-flip and false DURING it: between this transaction committing
--     and step 3 restarting the worker, `article_read` is still draining and still writing
--     routing_tags — and the subscriptions this file seeds are exactly what arms this trigger.
--     For those few minutes it would enqueue `s:transfer:…` against mig 175's `t:…` on the same
--     row: the churn loop, opened by the act that exists to close it. Dropping it here costs
--     nothing (Phase 9 was going to delete it) and closes the window.
--
-- Kept on purpose: `enqueue_voices_on_packet` (mig 206/212) — this is the new rail's waker.
--
-- ## What is seeded, and why FOUR rows
--
-- Two grains per voice, never '*': both triggers join `entity_type` on strict equality, so a
-- '*' row fans out to NOBODY (D-T15's correction — the Influencer would have silently never
-- woken). `player` and `team` are the two grains pipeline_work's CHECK admits.
--
--   * ('charged','vibe',player|team) — the Influencer's waker. 7.6 gated her Journalist-side
--     enqueue to legacy, so without these rows she has no waker at all on the packet rail.
--   * ('narratives','narratives',player|team) — the JOURNALIST's waker, and the row 7.4 did not
--     know it needed. Mig 206 arm 2 used to fan narratives unconditionally; mig 212 gated it on
--     a subscription at that grain (D-T14 resolution (b)) so packets could compile in shadow.
--     That gate is still in force after the flip, so the Journalist needs a row like everyone
--     else. Arm 2 does NOT read the tag column — it is an existence gate — and 'narratives' is
--     used as the tag because no packet routing_tag equals it, which keeps arm 1 inert for
--     these rows and leaves arm 2 the only reader.
--
-- The Insider's ('transfer','transfers','team') row is SAFE here and was not safe before: the
-- competing writer (mig 175) is dropped in this same transaction.
--
-- ## Rollback (the 7-day window §2 promises)
--
-- RAIL=legacy in both env files + restart, then re-create the three triggers from
-- sql/schema/schema.sql (they are in the snapshot committed alongside mig 212) and
-- DELETE FROM stage_routing_subscriptions. The packets already compiled stay — they are
-- append-only archive and harm nothing on the legacy rail.

BEGIN;

-- 1. The legacy rail's enqueue seams. DROP, not disable: a disabled trigger is a thing someone
--    re-enables by accident, and Appendix A's revert block is the documented way back.
DROP TRIGGER IF EXISTS enqueue_derive_on_vetted            ON public.news_article_entities;
DROP TRIGGER IF EXISTS enqueue_transfers_if_transfer_related ON public.news_articles;
DROP TRIGGER IF EXISTS enqueue_voices_on_routing_tags      ON public.news_articles;

-- 2. The packet rail's routing table. Data, never a model's decision (E1).
INSERT INTO public.stage_routing_subscriptions (tag, stage, entity_type, note) VALUES
    ('charged',    'vibe',       'player', 'PLAN-one-rail 7.4/7.6 — the Influencer wakes on a charged packet (E3, first-voice-capable).'),
    ('charged',    'vibe',       'team',   'PLAN-one-rail 7.4/7.6 — same, at team grain. Two rows because neither trigger reads ''*'' as a wildcard (D-T15).'),
    ('narratives', 'narratives', 'player', 'PLAN-one-rail 8.6 — the Journalist''s waker. Mig 212 made arm 2 subscription-gated, so he needs a row; arm 2 ignores the tag and reads only (stage, entity_type).'),
    ('narratives', 'narratives', 'team',   'PLAN-one-rail 8.6 — same, at team grain.'),
    ('transfer',   'transfers',  'team',   'PLAN-one-rail 7.4/7.5 — the Insider''s packet slice. Safe only because mig 175''s article-grain trigger is dropped in this same transaction (D-T15 resolution (a)).')
ON CONFLICT (tag, stage, entity_type) DO NOTHING;

-- 3. Assert the act actually happened, inside the transaction, before it commits (the 045
--    parity-gate habit). A silent no-op here is a flip that half-landed.
DO $$
DECLARE
    n_triggers int;
    n_subs     int;
BEGIN
    SELECT count(*) INTO n_triggers
      FROM pg_trigger
     WHERE NOT tgisinternal
       AND tgname IN ('enqueue_derive_on_vetted',
                      'enqueue_transfers_if_transfer_related',
                      'enqueue_voices_on_routing_tags');
    IF n_triggers <> 0 THEN
        RAISE EXCEPTION 'flip aborted: % legacy trigger(s) still present', n_triggers;
    END IF;

    SELECT count(*) INTO n_subs FROM public.stage_routing_subscriptions;
    IF n_subs < 5 THEN
        RAISE EXCEPTION 'flip aborted: stage_routing_subscriptions has % rows, expected >= 5', n_subs;
    END IF;

    IF NOT EXISTS (SELECT 1 FROM pg_trigger
                    WHERE NOT tgisinternal AND tgname = 'enqueue_voices_on_packet') THEN
        RAISE EXCEPTION 'flip aborted: the packet trigger is missing — the new rail has no waker';
    END IF;
END $$;

-- Self-record INSIDE the transaction so apply + record are atomic.
INSERT INTO public.schema_migrations(version) VALUES ('213_retire_legacy_rail')
    ON CONFLICT DO NOTHING;

COMMIT;

-- PLAN-one-rail 7.4 — seed stage_routing_subscriptions (packet-grain voice routing, E1).
--
-- ############################################################################
-- ##  DO NOT APPLY YET.  This file is PREPARED, not a migration.            ##
-- ##  Applying it today starts a live churn loop on the `transfers` stage.  ##
-- ############################################################################
--
-- ## Why it is held (measured 2026-08-05 22:5x EDT, archbox)
--
-- `stage_routing_subscriptions` is read by TWO triggers, not one:
--
--   * mig 206 `enqueue_voices_on_packet`  — AFTER INSERT ON packets   (inert: 0 packets, and
--     COGNITION_PACKET_COMPILE is off — D-T14)
--   * mig 197 `enqueue_voices_on_routing_tags` — AFTER UPDATE OF routing_tags ON news_articles
--     (LIVE: tgenabled='O', and `article_reader` writes routing_tags on every legacy read —
--     37,590 articles carry tags today)
--
-- 7.4's text assumes seeding only arms the packet trigger. It does not. The moment a
-- ('transfer','transfers','team') row exists, mig 197's trigger begins enqueueing
-- (transfers, team, id, sport) with input_version 's:transfer:<count>:<md5>' — onto the exact
-- same pipeline_work primary key that mig 175's still-live trigger writes as 't:<count>:<md5>'
-- (all 132 live transfers rows are 't:' today). Two writers, two prefixes, one row, and
-- `work::enqueue`'s ON CONFLICT reopens on any input_version change: the mig-197 churn loop,
-- arriving on the LEGACY rail, on a production stage — the same failure D-T14 parked for
-- packets, through the other door.
--
-- ## Preconditions (any one of these makes it safe; Scott's call — logged as D-T15)
--
--   (a) Phase 8 drops mig 175's `enqueue_transfers_if_transfer_related`, leaving one writer; or
--   (b) a migration makes mig 197's trigger ignore packet-era tags (e.g. gate it on a column
--       the Editor does not write), leaving the packet trigger as the only reader; or
--   (c) seed ONLY the ('charged','vibe',...) rows now and hold the transfers row until (a) —
--       `vibe` has no competing article-grain enqueue, so it carries no churn risk. This is
--       the cheap partial, and it is what 7.6 actually needs.
--
-- ## The '*' correction
--
-- 7.4 as written says ('charged','vibe','*'). Neither trigger treats '*' as a wildcard — both
-- join `se.entity_type = s.entity_type` (mig 206 line 52) / `te.entity_type = s.entity_type`
-- (mig 197 line 116) on strict equality, so a '*' row would fan out to NOBODY and the
-- Influencer would silently never wake. '*' is read here as its evident intent — every grain a
-- voice is keyed on — which is player and team, the two grains `pipeline_work`'s CHECK admits
-- (mig 206 excludes 'person' deliberately). Two rows, not one wildcard. Data-only, and
-- reversible with a DELETE.

BEGIN;

INSERT INTO public.stage_routing_subscriptions (tag, stage, entity_type, note) VALUES
    ('charged',  'vibe',      'player', 'PLAN-one-rail 7.4/7.6 — the Influencer wakes on a charged packet (E3, first-voice-capable).'),
    ('charged',  'vibe',      'team',   'PLAN-one-rail 7.4/7.6 — same, at team grain. Two rows because neither trigger reads ''*'' as a wildcard.'),
    ('transfer', 'transfers', 'team',   'PLAN-one-rail 7.4/7.5 — the Insider''s packet slice. HOLD until mig 175''s article-grain trigger is gone (Phase 8) — see the header.')
ON CONFLICT (tag, stage, entity_type) DO NOTHING;

COMMIT;

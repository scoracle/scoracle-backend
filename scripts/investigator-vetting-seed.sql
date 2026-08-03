-- Investigator NBA vetting seeds (PLAN-one-rail Phase 5, Scott's 2026-08-03 ruling).
-- Two blocks; run the SMOKE block first, eyeball the writes (investigator_review_accepted,
-- entity_facts, players), then run the FULL block when satisfied.
--
-- Work is claimed only where COGNITION_STAGES includes investigate_entity (the 5.9
-- deploy, or a bounded manual worker run) — seeding ahead of that just parks pending rows.

-- ============================== SMOKE (bounded: 5 items) ==============================
BEGIN;

-- Three headliner NBA players missing dob/photo (deterministic pick: alphabetical).
INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
SELECT 'investigate_entity', 'player', p.id, 'NBA', 'pending', NOW(), NOW()
FROM players p
WHERE p.sport = 'NBA' AND p.tier = 'headliner'
  AND (p.date_of_birth IS NULL OR p.photo_url IS NULL)
ORDER BY p.name
LIMIT 3
ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING;

-- Two known NBA head coaches as mystery candidates (the non-player/team test class).
-- Seeded exactly as the Editor's sweep would have: person-kind, descriptor carried on a
-- mention-less candidate (the sweep's mention row needs an article; these are hand-seeds).
INSERT INTO entity_candidates (idempotency_key, norm_name, kind_hint, sport, state, mention_count)
VALUES ('nba:' || public.nrm('Erik Spoelstra'), public.nrm('Erik Spoelstra'), 'person', 'NBA', 'pending', 1),
       ('nba:' || public.nrm('Steve Kerr'),     public.nrm('Steve Kerr'),     'person', 'NBA', 'pending', 1)
ON CONFLICT (idempotency_key) DO UPDATE SET state = 'pending', last_seen_at = NOW();

INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
SELECT 'investigate_entity', 'candidate', c.id, 'NBA', 'pending', NOW(), NOW()
FROM entity_candidates c
WHERE c.idempotency_key IN ('nba:' || public.nrm('Erik Spoelstra'),
                            'nba:' || public.nrm('Steve Kerr'))
ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING;

COMMIT;

-- ============================== FULL (run after the smoke passes review) ==============
-- BEGIN;
-- -- Every active-tier NBA player with a metadata gap (~603 rows; Wikimedia-budgeted at
-- -- ~4 requests each with 7-day caching ≈ a few hours of polite single-file fetching).
-- INSERT INTO pipeline_work (stage, entity_type, entity_id, sport, status, available_at, updated_at)
-- SELECT 'investigate_entity', 'player', p.id, 'NBA', 'pending', NOW(), NOW()
-- FROM players p
-- WHERE p.sport = 'NBA' AND p.tier IN ('headliner', 'starter', 'bench')
--   AND (p.date_of_birth IS NULL OR p.photo_url IS NULL OR p.weight IS NULL)
-- ON CONFLICT (stage, entity_type, entity_id, sport) DO NOTHING;
-- COMMIT;

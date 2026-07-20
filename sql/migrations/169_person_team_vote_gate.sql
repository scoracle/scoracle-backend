-- 169_person_team_vote_gate.sql
--
-- Junction memory rollout step 7, deferred half: the TEAM TOKEN joins the person
-- promotion pass/fail gate (operator, 2026-07-20). A candidate whose article mentions
-- DISAGREE on which team they are tied to is ambiguous — a merged same-name pair, or a
-- multi-club agent — and must not earn entityhood on raw count alone.
--
-- (1) narrative_person_mentions gains team_context_id — the per-mention team vote the
--     graph stage already extracts (GraphPerson.team_context_id) but previously
--     discarded after a first-write-wins set on the person row. Now it is recorded per
--     mention so votes accumulate and can be judged for consistency.
--
-- (2) promote_narrative_persons v2 adds the team-token gate to the existing count
--     criteria (>=2 sources, >=3 mentions, >=2 days span):
--       - among mentions WITH a team vote, the modal team must hold a clear majority
--         (>= 60% of voting mentions) — a split vote holds the candidate;
--       - candidates with NO team votes pass this criterion vacuously (a team-less
--         person — pundit, free-agent's agent — is still a valid entity, it just never
--         earns a "Team figure:" card line);
--       - on promotion, team_id is set to the modal team (the vote decides the tie,
--         replacing mig-166's first-write-wins provisional).
--
-- ADDITIVE — apply before deploying the binary that writes the vote (accumulate_person).
-- Existing mentions carry NULL votes, so no candidate is retroactively gate-failed; the
-- gate tightens only as fresh voted mentions accrue.

BEGIN;

ALTER TABLE public.narrative_person_mentions
    ADD COLUMN IF NOT EXISTS team_context_id integer;

COMMENT ON COLUMN public.narrative_person_mentions.team_context_id IS
    'Per-mention team vote from the graph extraction (the listed team the person was '
    'tied to in THIS article), or NULL. Aggregated by promote_narrative_persons for the '
    'team-token consistency gate.';

CREATE OR REPLACE FUNCTION public.promote_narrative_persons(p_sport text)
RETURNS integer
    LANGUAGE sql
    AS $$
WITH cand AS (
    -- Count-eligible candidates (mig 166 criteria).
    SELECT p.id
    FROM narrative_persons p
    WHERE p.sport = p_sport
      AND p.status = 'candidate'
      AND p.merged_into IS NULL
      AND p.distinct_sources >= 2
      AND p.mention_count >= 3
      AND p.last_seen_at - p.first_seen_at >= interval '2 days'
),
votes AS (
    SELECT m.person_id,
           count(*) FILTER (WHERE m.team_context_id IS NOT NULL) AS voting_mentions,
           mode() WITHIN GROUP (ORDER BY m.team_context_id)
               FILTER (WHERE m.team_context_id IS NOT NULL) AS modal_team
    FROM public.narrative_person_mentions m
    WHERE m.sport = p_sport AND m.person_id IN (SELECT id FROM cand)
    GROUP BY m.person_id
),
detail AS (
    SELECT c.id AS person_id,
           COALESCE(v.voting_mentions, 0) AS voting_mentions,
           v.modal_team,
           COALESCE((SELECT count(*) FROM public.narrative_person_mentions m2
                      WHERE m2.person_id = c.id AND m2.sport = p_sport
                        AND m2.team_context_id = v.modal_team), 0) AS modal_votes
    FROM cand c
    LEFT JOIN votes v ON v.person_id = c.id
),
eligible AS (
    -- The team-token gate: no votes ⇒ vacuous pass (team-less person); votes present ⇒
    -- the modal team must hold a clear majority (a split vote is ambiguous, held).
    SELECT person_id, modal_team
    FROM detail
    WHERE voting_mentions = 0
       OR modal_votes::numeric / voting_mentions >= 0.6
),
promoted AS (
    UPDATE public.narrative_persons p
       SET status = 'active',
           team_id = COALESCE(e.modal_team, p.team_id),
           updated_at = NOW()
      FROM eligible e
     WHERE e.person_id = p.id
    RETURNING p.id
)
SELECT count(*)::integer FROM promoted;
$$;

COMMENT ON FUNCTION public.promote_narrative_persons(text) IS
    'Nightly candidate->active promotion on accumulated evidence (>=2 sources, >=3 '
    'mentions, >=2 days span) PLUS a team-token consistency gate (mig 169): mentions '
    'that vote on a team must show a clear-majority (>=60%) modal team, else the '
    'candidate is held; the modal team becomes the person''s team_id. Team-less '
    'candidates pass the token gate vacuously.';

INSERT INTO public.schema_migrations(version) VALUES ('169_person_team_vote_gate')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

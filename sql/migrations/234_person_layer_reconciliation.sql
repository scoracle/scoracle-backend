-- 234_person_layer_reconciliation.sql
--
-- Appendix B, finally written: the bridge between the two person layers.
--
-- Scott, 2026-09-04: "It's imperative that we are able to add new entities and
-- entity types in as they're unearthed… coaches, owners, agents, etc so that the
-- story becomes richer and richer and the context grows better and better."
--
-- The unearthing already exists — twice, in parallel, unreconciled since mig 203
-- promised "that graph-layer table is unaffected and reconciles later (Appendix B)":
--
--   * narrative_persons (graph stage): evidence-accumulating — mentions, distinct
--     sources, per-mention team votes, promotion gate — but unlinkable. Feeds one
--     "Team figure:" memory line and nothing else.
--   * public.persons (Investigator): verified and resolvable — Wikidata/Wikipedia
--     two-arm gate, aliases mirrored into entity_name_surfaces — but evidence-blind.
--     A person accrues one role fact at accept time and never grows.
--
-- A coach discovered by graph never became a persons row; a coach accepted by the
-- Investigator never gained the graph's mention evidence. The growth didn't compound.
--
-- The bridge is a nightly reconcile, riding the same cron chain as the promotion it
-- follows (cron-narrative-links.sh, after promote_narrative_persons):
--
--   1. LINK — an active narrative_person whose nrm(name) uniquely matches a verified
--      person surface gets person_id set. The graph evidence and the verified
--      identity become one figure.
--   2. NOMINATE — an active narrative_person with no verified match enters the
--      STANDING Investigator path: entity_candidates upsert (same idempotency key,
--      same 30-day reopen semantics as the Editor's sweep — candidates.rs is the
--      reference implementation), its graph mentions carried across as
--      candidate_mentions evidence (descriptor = the graph kind + modal team), and
--      an investigate_entity enqueue. No new verifier, no new stage: graph evidence
--      flows into the same two-arm gate everything else passes.
--
-- The loop then closes by itself: acceptance writes persons + entity_name_surfaces,
-- and the NEXT night's step 1 links the narrative_person. Discovery → evidence →
-- verification → identity → linkage, all on existing rails.
--
-- Deliberately NOT here: person cards, person fan-out, person profile routes. The
-- role fence and mig 206's player/team fan-out stand. Persons enrich player and
-- team stories (their kinds now travel in packet casts — packet.rs, same release);
-- they do not become card-bearing entities. That is the lean line.
--
-- Volume note: narrative_persons was ~2.4k rows (PLAN-one-rail:1340), of which only
-- the promoted actives nominate — a one-time bulk into an investigate_entity queue
-- that already runs a drain deficit (D-T10). These arrive pre-gated (≥2 sources,
-- ≥3 mentions, ≥2-day span, team vote), so they are the highest-quality nominations
-- the queue has ever received; FIFO handles the rest.

BEGIN;

-- The link. No FK by design — the substrate tables carry loose triples (mig 205),
-- and a merged/retired person must not block persons maintenance.
ALTER TABLE public.narrative_persons
    ADD COLUMN IF NOT EXISTS person_id integer;

COMMENT ON COLUMN public.narrative_persons.person_id IS
    'Reconciliation bridge (mig 234, the Appendix B mig 203 deferred): the verified public.persons row this graph-layer figure resolved to. Set nightly by reconcile_narrative_persons() on a unique nrm(name) surface match; NULL means unverified (and, if active, nominated to the Investigator). The graph side keeps accumulating evidence either way.';

CREATE INDEX IF NOT EXISTS idx_narrative_persons_person
    ON public.narrative_persons (person_id)
    WHERE person_id IS NOT NULL;

CREATE OR REPLACE FUNCTION public.reconcile_narrative_persons(p_sport text)
RETURNS TABLE(linked integer, nominated integer)
LANGUAGE plpgsql
AS $$
DECLARE
    v_linked integer := 0;
    v_nominated integer := 0;
BEGIN
    -- (1) LINK: unique verified-surface match on the one normalizer (mig 198:
    -- the database owns nrm()). Ambiguous names (two verified persons sharing a
    -- normalized name) stay unlinked — the Investigator's resolve-to-existing is
    -- the only judge of that tie, and it gets the case via step 2.
    UPDATE public.narrative_persons p
       SET person_id = m.person_id, updated_at = NOW()
      FROM (
        SELECT np.id AS np_id, MIN(ens.entity_id) AS person_id
          FROM public.narrative_persons np
          JOIN public.entity_name_surfaces ens
            ON ens.entity_type = 'person'
           AND (ens.sport = p_sport OR ens.sport IS NULL)
           AND ens.norm = public.nrm(np.name)
         WHERE np.sport = p_sport
           AND np.status = 'active'
           AND np.merged_into IS NULL
           AND np.person_id IS NULL
         GROUP BY np.id
        HAVING COUNT(DISTINCT ens.entity_id) = 1
      ) m
     WHERE p.id = m.np_id;
    GET DIAGNOSTICS v_linked = ROW_COUNT;

    -- (2) NOMINATE the still-unlinked actives through the standing path. Mirrors
    -- candidates.rs::nominate_one exactly: same key, same kind_hint COALESCE, same
    -- 30-day reopen; evidence rows dedupe on (candidate_id, article_id) so the
    -- nightly rerun is a no-op once carried. The enqueue mirrors work::enqueue
    -- (mig 225 FIFO preservation: a pending row keeps its place in line).
    WITH nominees AS (
        SELECT np.id, np.name, np.kind, np.team_id
          FROM public.narrative_persons np
         WHERE np.sport = p_sport
           AND np.status = 'active'
           AND np.merged_into IS NULL
           AND np.person_id IS NULL
    ),
    upserted AS (
        INSERT INTO public.entity_candidates
            (idempotency_key, norm_name, kind_hint, sport, state, first_seen_at, last_seen_at)
        SELECT DISTINCT ON (public.nrm(n.name))
               lower(p_sport) || ':' || public.nrm(n.name), public.nrm(n.name),
               'person', p_sport, 'pending', NOW(), NOW()
          FROM nominees n
         ORDER BY public.nrm(n.name), n.id
        ON CONFLICT (idempotency_key) DO UPDATE SET
            last_seen_at = NOW(),
            kind_hint = COALESCE(public.entity_candidates.kind_hint, EXCLUDED.kind_hint),
            state = CASE
                WHEN public.entity_candidates.state NOT IN ('pending', 'accepted')
                 AND public.entity_candidates.decided_at IS NOT NULL
                 AND public.entity_candidates.decided_at < NOW() - interval '30 days'
                THEN 'pending'
                ELSE public.entity_candidates.state
            END
        RETURNING id, norm_name, state
    ),
    evidence AS (
        INSERT INTO public.candidate_mentions
            (candidate_id, article_id, quote, editor_descriptor, observed_at)
        SELECT u.id, m.article_id, NULL,
               n.kind || COALESCE(', ' || t.name, ''),
               NOW()
          FROM upserted u
          JOIN nominees n ON public.nrm(n.name) = u.norm_name
          JOIN public.narrative_person_mentions m
            ON m.person_id = n.id AND m.sport = p_sport
          LEFT JOIN public.teams t
            ON t.id = n.team_id AND t.sport = p_sport
        ON CONFLICT (candidate_id, article_id) DO NOTHING
        RETURNING candidate_id
    ),
    counted AS (
        UPDATE public.entity_candidates c
           SET mention_count = c.mention_count + e.n
          FROM (SELECT candidate_id, COUNT(*) AS n FROM evidence GROUP BY 1) e
         WHERE c.id = e.candidate_id
        RETURNING c.id
    ),
    enq AS (
        INSERT INTO public.pipeline_work
            (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
        SELECT 'investigate_entity', 'candidate', u.id, p_sport, 'pending', NULL, NOW(), NOW()
          FROM upserted u
         WHERE u.state = 'pending'
        ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
            status       = 'pending',
            attempts     = 0,
            available_at = CASE WHEN public.pipeline_work.status = 'pending'
                                THEN public.pipeline_work.available_at
                                ELSE NOW() END,
            updated_at   = NOW(),
            last_error   = NULL
        WHERE public.pipeline_work.status = 'failed'
        RETURNING entity_id
    )
    SELECT COUNT(*)::integer INTO v_nominated FROM enq;

    IF v_nominated > 0 THEN
        PERFORM pg_notify('pipeline_work_ready', '');
    END IF;

    RETURN QUERY SELECT v_linked, v_nominated;
END;
$$;

COMMENT ON FUNCTION public.reconcile_narrative_persons(text) IS
    'The mig 234 bridge, nightly after promote_narrative_persons (cron-narrative-links.sh): link active graph figures to verified persons on a unique surface match; nominate the rest into the standing Investigator path with their graph evidence carried as candidate_mentions. Acceptance writes the surfaces that make the next night''s link succeed — the loop closes itself.';

COMMIT;

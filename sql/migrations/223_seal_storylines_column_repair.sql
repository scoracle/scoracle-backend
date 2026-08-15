-- 223 — seal_storylines: the resolved CTE names its column, the seal finally lands
--
-- Mig 219's seal_storylines returned `s.id` from the resolved CTE while the D5 edge
-- close read `r.storyline_id` — every nightly invocation since the collapse errored at
-- that line, so no storyline has EVER ground-truth-resolved, and (because the cron runs
-- one psql chain under ON_ERROR_STOP) every step after the seal — part promotion,
-- source performance, person promotion — silently never ran. Surfaced by 222's
-- demolition run: the repointed memory cards read resolved/dormant storylines, which
-- made the empty resolved set worth explaining.
--
-- One column alias. Everything else is byte-identical to the 219 definition.

CREATE OR REPLACE FUNCTION public.seal_storylines(p_sport text)
 RETURNS integer
 LANGUAGE plpgsql
AS $fn$
DECLARE
    v_resolved integer := 0;
BEGIN
    -- Ground truth -> resolved (the thread seal's resolved arm, rebuilt on
    -- storylines). An OPEN storyline with a transfer-flavored member and an
    -- applied ground-truth move since it opened resolves: status flips, and D5
    -- happens in the same stroke — the move's player keeps the part, every
    -- other active edge closes as not_the_outcome. Transfer flavor reads
    -- routing_tags (the Editor-derived fact) with the legacy bucket as
    -- fallback for pre-flip articles. Dormancy (the thread seal's faded arm)
    -- is already covered by mark_dormant() in the worker: a 14d-quiet
    -- storyline leaves the candidate set AND the memory card (open-only).
    WITH hits AS (
        SELECT DISTINCT ON (s.id)
               s.id AS storyline_id, g.player_id, g.team_id, g.applied_at
        FROM public.storylines s
        JOIN public.storyline_articles sa ON sa.storyline_id = s.id
        JOIN public.news_articles a ON a.id = sa.article_id
        JOIN public.storyline_entities se
          ON se.storyline_id = s.id AND se.left_at IS NULL
        JOIN public.transfer_ground_truth g
          ON g.sport = s.sport
         AND g.applied_at > s.first_seen_at
         AND ((se.entity_type = 'player' AND g.player_id = se.entity_id)
           OR (se.entity_type = 'team' AND g.team_id = se.entity_id))
        WHERE s.sport = p_sport
          AND s.status = 'open'
          AND (a.bucket = 'transfer' OR a.routing_tags @> ARRAY['transfer'])
        ORDER BY s.id, g.applied_at DESC
    ),
    resolved AS (
        UPDATE public.storylines s
           SET status = 'resolved',
               resolved_at = h.applied_at,
               resolution = jsonb_build_object(
                   'outcome', 'transfer_confirmed',
                   'player_id', h.player_id,
                   'team_id', h.team_id,
                   'sealed_by', 'seal_storylines')
          FROM hits h
         WHERE s.id = h.storyline_id
         RETURNING s.id AS storyline_id, h.player_id
    ),
    -- Data-modifying CTEs run exactly once and to completion, so the edge
    -- close lands in the same statement (and the same snapshot) as the
    -- resolve — one stroke, as D5 requires.
    closed AS (
        UPDATE public.storyline_entities se
           SET left_at = now(), exit_reason = 'not_the_outcome'
          FROM resolved r
         WHERE se.storyline_id = r.storyline_id
           AND se.left_at IS NULL
           AND NOT (se.entity_type = 'player' AND se.entity_id = r.player_id)
         RETURNING 1
    )
    SELECT count(*) INTO v_resolved FROM resolved;

    RETURN v_resolved;
END;
$fn$;

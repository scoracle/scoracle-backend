BEGIN;
SET LOCAL lock_timeout = '2s';

ALTER TABLE public.news_summaries
    ADD COLUMN scoring_version text,
    DROP CONSTRAINT news_summaries_card_score_check,
    ADD CHECK (card_score BETWEEN 0 AND 100);
ALTER TABLE public.insider_scores
    ADD COLUMN scoring_version text,
    DROP CONSTRAINT insider_scores_score_check,
    DROP CONSTRAINT insider_scores_previous_score_check,
    ADD CHECK (score BETWEEN 0 AND 100),
    ADD CHECK (previous_score BETWEEN 0 AND 100);

ALTER TABLE public.stat_summaries
    ADD COLUMN card_score numeric(4,1) CHECK (card_score BETWEEN 0 AND 100);
COMMENT ON COLUMN public.stat_summaries.card_score IS
    'Scout performance from the profile used to write this card: team rating_rank or player rating_score.';
COMMENT ON COLUMN public.news_summaries.card_score IS
    'Journalist coverage activity: ten points per distinct admitted report, up to 100, for new cards.';
COMMENT ON COLUMN public.insider_scores.score IS
    'Insider coverage activity: ten points per distinct admitted personnel report, up to 100, for new cards.';

-- Re-expand the same qualified projection to include the new score.
CREATE OR REPLACE VIEW public.qualified_stat_summaries AS
SELECT p.* FROM public.stat_summaries p
WHERE ((NOT (SELECT enabled FROM public.coverage_schedule) OR public.published_edition() IS NULL)
       AND p.candidate_edition IS NULL)
   OR (p.candidate_edition <= public.published_edition()
       AND EXISTS (SELECT 1 FROM public.coverage_editions e
                   WHERE e.edition_date = p.candidate_edition AND e.status = 'complete')
       AND public.coverage_carry_allowed(p.candidate_edition, 'rating', p.entity_type, p.entity_id, p.sport));

INSERT INTO public.schema_migrations(version) VALUES ('334_character_card_scores') ON CONFLICT DO NOTHING;
COMMIT;

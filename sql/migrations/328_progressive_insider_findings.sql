BEGIN;
-- Insider findings follow the assignment that produced them, which may target a team.
CREATE OR REPLACE VIEW public.qualified_transfer_rumors AS SELECT p.* FROM public.transfer_rumors p WHERE
 ((NOT (SELECT enabled FROM public.coverage_schedule) OR public.published_edition() IS NULL) AND p.candidate_edition IS NULL) OR
 (p.candidate_edition <= public.published_edition() AND EXISTS(SELECT 1 FROM public.coverage_editions e
  WHERE e.edition_date=p.candidate_edition AND e.status='complete') AND
 public.coverage_carry_allowed(p.candidate_edition,'transfers',p.candidate_entity_type,p.candidate_entity_id,p.sport)) OR
 public.coverage_character_visible(p.candidate_edition,'transfers',p.candidate_entity_type,p.candidate_entity_id,p.sport);
UPDATE public.coverage_publication SET revision=revision+1;
INSERT INTO public.schema_migrations(version) VALUES('328_progressive_insider_findings') ON CONFLICT DO NOTHING;
COMMIT;

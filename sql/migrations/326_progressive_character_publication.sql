-- Publish the three news characters when their own validated assignment completes.
BEGIN;
CREATE FUNCTION public.coverage_character_visible(candidate date, task text, subject_type text, subject_id bigint, league text)
RETURNS boolean LANGUAGE sql STABLE AS $$
 SELECT EXISTS (
  SELECT 1 FROM public.coverage_assignments a
  WHERE a.edition_date=candidate AND a.stage=task AND a.entity_type=subject_type
   AND a.entity_id=subject_id AND a.sport=league
   AND a.outcome='completed' AND a.resolution='candidate'
 ) AND (NULLIF(current_setting('scoracle.reporting_date',true),'') IS NOT NULL OR NOT EXISTS (
  SELECT 1 FROM public.coverage_assignments a
  WHERE a.edition_date>candidate AND a.stage=task AND a.entity_type=subject_type
   AND a.entity_id=subject_id AND a.sport=league AND a.outcome='completed'
   AND a.resolution='abstained'
 ))
$$;
DO $$ DECLARE t text; task text; BEGIN
 FOREACH t IN ARRAY ARRAY['news_summaries','vibe_scores','insider_scores'] LOOP
  task:=CASE t WHEN 'news_summaries' THEN 'narratives' WHEN 'vibe_scores' THEN 'vibe' ELSE 'transfers' END;
  EXECUTE format('CREATE OR REPLACE VIEW public.qualified_%I AS SELECT p.* FROM public.%I p WHERE
   ((NOT (SELECT enabled FROM public.coverage_schedule) OR public.published_edition() IS NULL) AND p.candidate_edition IS NULL) OR
   (p.candidate_edition <= public.published_edition() AND EXISTS(SELECT 1 FROM public.coverage_editions e
      WHERE e.edition_date=p.candidate_edition AND e.status=''complete'') AND
    public.coverage_carry_allowed(p.candidate_edition,%L,p.entity_type,p.entity_id,p.sport)) OR
   public.coverage_character_visible(p.candidate_edition,%L,p.entity_type,p.entity_id,p.sport)',t,t,task,task);
 END LOOP;
END $$;
CREATE TRIGGER invalidate_character_completion AFTER UPDATE OF outcome,resolution ON public.coverage_assignments
 FOR EACH ROW WHEN (NEW.stage IN ('narratives','vibe','transfers') AND
 (OLD.outcome IS DISTINCT FROM NEW.outcome OR OLD.resolution IS DISTINCT FROM NEW.resolution))
 EXECUTE FUNCTION public.invalidate_coverage_publication();
CREATE TRIGGER invalidate_character_insertion AFTER INSERT ON public.coverage_assignments
 FOR EACH ROW WHEN (NEW.stage IN ('narratives','vibe','transfers') AND NEW.outcome='completed')
 EXECUTE FUNCTION public.invalidate_coverage_publication();
CREATE TRIGGER invalidate_character_withdrawal AFTER DELETE ON public.coverage_assignments
 FOR EACH ROW WHEN (OLD.stage IN ('narratives','vibe','transfers') AND OLD.outcome='completed')
 EXECUTE FUNCTION public.invalidate_coverage_publication();
UPDATE public.coverage_publication SET revision=revision+1;
INSERT INTO public.schema_migrations(version) VALUES('326_progressive_character_publication') ON CONFLICT DO NOTHING;
COMMIT;

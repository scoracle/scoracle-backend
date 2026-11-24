-- Run after migration 335. Exercise the same function as the card writer and scoreboard.
BEGIN;
UPDATE public.coverage_schedule SET enabled=false;
DO $$
DECLARE
    day date := public.character_reporting_date();
    active_season integer := (SELECT current_season FROM public.sports WHERE id='NBA');
    target_id constant integer := -335;
    changes record;
BEGIN
    PERFORM set_config('scoracle.reporting_date', (day - 1)::text, true);
    INSERT INTO public.stat_summaries
        (entity_type, entity_id, sport, season, trigger_type, body, notability, card_score, generated_at)
    VALUES ('team',target_id,'NBA',active_season,'manual','Earlier profile',90,60,now()-interval '1 day');
    INSERT INTO public.vibe_scores
        (entity_type,entity_id,sport,trigger_type,model_version,prompt_version,prompt,sentiment,generated_at)
    VALUES ('team',target_id,'NBA','periodic','test','test','Earlier tone',50,now()-interval '1 day');

    PERFORM set_config('scoracle.reporting_date', day::text, true);
    INSERT INTO public.stat_summaries
        (entity_type, entity_id, sport, season, trigger_type, body, notability, card_score, generated_at)
    VALUES ('team',target_id,'NBA',active_season,'manual','Superseded',10,30,now()-interval '1 minute'),
           ('team',target_id,'NBA',active_season,'manual','Today',20,68,now()-interval '1 second'),
           ('team',target_id,'NBA',active_season-1,'manual','Other season',20,7,now());
    INSERT INTO public.vibe_scores
        (entity_type,entity_id,sport,trigger_type,model_version,prompt_version,prompt,sentiment,generated_at)
    VALUES ('team',target_id,'NBA','periodic','test','test','Today',38,now());
    -- Pre-schedule cards remain published after enrollment, even without a publication pointer.
    UPDATE public.coverage_schedule SET enabled=true,enrolled_at=now();
    SELECT * INTO STRICT changes FROM public.analyst_card_changes('NBA','team',target_id);
    IF (changes.rating_slope, changes.vibe_slope, changes.momentum_score)
        IS DISTINCT FROM ROW(8::numeric,-12::numeric,-2::numeric) THEN
        RAISE EXCEPTION 'Expected performance +8 and coverage tone -12, got %', changes;
    END IF;

    UPDATE public.stat_summaries SET card_score=NULL WHERE sport='NBA' AND entity_id=target_id;
    UPDATE public.vibe_scores SET sentiment=50 WHERE sport='NBA' AND entity_id=target_id;
    SELECT * INTO STRICT changes FROM public.analyst_card_changes('NBA','team',target_id);
    IF changes.rating_slope IS NOT NULL OR changes.momentum_score IS DISTINCT FROM 0::numeric THEN
        RAISE EXCEPTION 'Unscored Scout and flat coverage tone, got %', changes;
    END IF;
    DELETE FROM public.vibe_scores WHERE sport='NBA' AND entity_id=target_id AND reporting_date=day-1;
    IF EXISTS(SELECT 1 FROM public.analyst_card_changes('NBA','team',target_id)) THEN
        RAISE EXCEPTION 'One scored reporting day cannot establish momentum';
    END IF;
END $$;
ROLLBACK;

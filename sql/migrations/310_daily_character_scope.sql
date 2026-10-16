BEGIN;
-- Keep historical week stamps and calendar rows; no new output or job uses them.
CREATE FUNCTION public.stamp_card_day() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE cutoff_at timestamptz;
BEGIN
    IF NEW.reporting_date IS NULL THEN
        IF to_jsonb(NEW)->>'reporting_start' IS NOT NULL THEN
            NEW.reporting_date:=((to_jsonb(NEW)->>'reporting_start')::timestamptz AT TIME ZONE 'America/Detroit')::date;
        ELSE
            SELECT e.cutoff_at INTO cutoff_at FROM public.coverage_schedule s
            JOIN public.coverage_editions e USING(edition_date) WHERE s.enabled AND s.phase='characters';
            NEW.reporting_date:=COALESCE((cutoff_at AT TIME ZONE 'America/Detroit')::date-1,
                (NEW.generated_at AT TIME ZONE 'America/Detroit')::date);
        END IF;
    END IF;
    NEW.week_season:=NULL;
    NEW.week_no:=NULL;
    RETURN NEW;
END;
$$;
DO $$ DECLARE t text; BEGIN
    FOREACH t IN ARRAY ARRAY['news_summaries','vibe_scores','insider_scores','transfer_rumors',
        'momentum_summaries','stat_summaries','sigil_synthesis','rating_history','momentum_scores'] LOOP
        EXECUTE format('DROP TRIGGER IF EXISTS stamp_week_on_insert ON public.%I',t);
        EXECUTE format('ALTER TABLE public.%I ADD COLUMN reporting_date date',t);
        EXECUTE format('UPDATE public.%I SET reporting_date=(generated_at AT TIME ZONE ''America/Detroit'')::date',t);
        EXECUTE format('ALTER TABLE public.%I ALTER COLUMN reporting_date SET NOT NULL',t);
        EXECUTE format('CREATE TRIGGER stamp_day_on_insert BEFORE INSERT ON public.%I FOR EACH ROW EXECUTE FUNCTION public.stamp_card_day()',t);
        EXECUTE format('CREATE INDEX idx_%s_day ON public.%I(sport,reporting_date)',t,t);
    END LOOP;
END; $$;
ALTER TABLE public.vibe_scores DROP CONSTRAINT vibe_period_card_check;
ALTER TABLE public.vibe_scores ADD CONSTRAINT vibe_daily_card_check CHECK (
    scoring_version IS NULL OR (sentiment IS NOT NULL AND hook IS NOT NULL
        AND length(btrim(hook)) BETWEEN 1 AND 140 AND prompt IS NOT NULL AND length(btrim(prompt))>0
        AND input_hash IS NOT NULL AND evidence_cutoff IS NOT NULL
        AND reporting_start IS NOT NULL AND reporting_end IS NOT NULL
        AND reporting_start=reporting_date::timestamp AT TIME ZONE 'America/Detroit'
        AND reporting_end=(reporting_date+1)::timestamp AT TIME ZONE 'America/Detroit')) NOT VALID;
DROP INDEX IF EXISTS public.vibe_card_period;
DROP FUNCTION public.seal_weeks(text);
DROP FUNCTION public.stamp_card_week();
DROP FUNCTION public.restamp_card_weeks();
DROP FUNCTION public.rebuild_season_weeks(text);
DROP FUNCTION public.week_of(text,timestamptz);
COMMENT ON TABLE public.season_weeks IS 'Retained historical reporting calendar. Daily outputs do not read or rebuild it. Fixture and NFL week facts remain in their original tables.';
COMMENT ON TABLE public.week_seals IS 'Retained historical weekly closure receipts. Weekly wrap generation is retired.';
CREATE OR REPLACE FUNCTION public.refresh_momentum_scores(p_sport text DEFAULT NULL::text)
RETURNS integer LANGUAGE plpgsql AS $$
DECLARE inserted_count integer;
BEGIN
    IF NOT pg_try_advisory_xact_lock(hashtext('refresh_momentum_scores')) THEN RETURN NULL; END IF;
    WITH target_sports AS (
        SELECT id AS sport,current_season FROM sports WHERE p_sport IS NULL OR id=upper(p_sport)
    ), observations AS (
        SELECT 'rating' rail,s.entity_type,s.entity_id,s.sport,s.id,s.generated_at,s.reporting_date,
            s.notability::numeric value FROM stat_summaries s JOIN target_sports t ON t.sport=s.sport
        UNION ALL
        SELECT 'vibe',s.entity_type,s.entity_id,s.sport,s.id,s.generated_at,s.reporting_date,
            s.sentiment::numeric FROM vibe_scores s JOIN target_sports t ON t.sport=s.sport
    ), daily AS (
        SELECT DISTINCT ON(rail,entity_type,entity_id,sport,reporting_date) * FROM observations
        WHERE generated_at<=now() AND reporting_date >= (now() AT TIME ZONE 'America/Detroit')::date-21
        ORDER BY rail,entity_type,entity_id,sport,reporting_date,generated_at DESC,id DESC
    ), ranked AS (
        SELECT *,row_number() OVER(PARTITION BY rail,entity_type,entity_id,sport ORDER BY reporting_date DESC) rank
        FROM daily WHERE value IS NOT NULL
    ), studied AS (
        SELECT rail,entity_type,entity_id,sport,
            (array_agg(value ORDER BY reporting_date DESC))[1]-(array_agg(value ORDER BY reporting_date))[1] slope,
            count(*)::int samples,min(generated_at) window_start,max(generated_at) window_end
        FROM ranked WHERE rank<=2 GROUP BY rail,entity_type,entity_id,sport HAVING count(*)=2
    ), vibe AS (
        SELECT entity_type,entity_id,sport,slope vibe_slope,samples vibe_samples,
            window_start vibe_window_start,window_end vibe_window_end FROM studied WHERE rail='vibe'
    ), rating AS (
        SELECT s.entity_type,s.entity_id,s.sport,t.current_season season,s.slope rating_slope,s.samples rating_samples,
            s.window_start rating_window_start,s.window_end rating_window_end
        FROM studied s JOIN target_sports t ON t.sport=s.sport WHERE rail='rating'
    ),
    entity_scores AS (
        SELECT COALESCE(v.entity_type, r.entity_type) AS entity_type,
               COALESCE(v.entity_id, r.entity_id) AS entity_id,
               COALESCE(v.sport, r.sport) AS sport,
               r.season,
               v.vibe_slope, COALESCE(v.vibe_samples, 0) AS vibe_samples,
               v.vibe_window_start, v.vibe_window_end,
               r.rating_slope, COALESCE(r.rating_samples, 0) AS rating_samples,
               r.rating_window_start, r.rating_window_end
        FROM vibe v
        FULL OUTER JOIN rating r
          ON r.entity_type = v.entity_type
         AND r.entity_id = v.entity_id
         AND r.sport = v.sport
    ),
    enriched AS (
        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               pci.league_id, pci.team_id, pci.position,
               COALESCE(pci.position_group, public.position_group(es.sport, pci.position)) AS position_group,
               t.conference, t.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        LEFT JOIN public.player_current_identity pci
          ON pci.player_id = es.entity_id AND pci.sport = es.sport AND es.entity_type = 'player'
        LEFT JOIN public.teams t
          ON t.id = pci.team_id AND t.sport = es.sport
        WHERE es.entity_type = 'player'

        UNION ALL

        SELECT es.sport, es.entity_type, es.entity_id,
               COALESCE(es.season, ts.current_season) AS season,
               tm.league_id, tm.id AS team_id, NULL::text AS position, NULL::text AS position_group,
               tm.conference, tm.division,
               es.vibe_slope, es.vibe_samples, es.vibe_window_start, es.vibe_window_end,
               es.rating_slope, es.rating_samples, es.rating_window_start, es.rating_window_end
        FROM entity_scores es
        JOIN target_sports ts ON ts.sport = es.sport
        JOIN public.teams tm
          ON tm.id = es.entity_id AND tm.sport = es.sport
        WHERE es.entity_type = 'team'
    )
    INSERT INTO public.momentum_scores (
        sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
        conference, division, vibe_slope, vibe_samples, vibe_window_start, vibe_window_end,
        rating_slope, rating_samples, rating_window_start, rating_window_end, momentum_score
    )
    SELECT sport, entity_type, entity_id, season, league_id, team_id, position, position_group,
           conference, division,
           round(vibe_slope, 3), vibe_samples, vibe_window_start, vibe_window_end,
           round(rating_slope, 3), rating_samples, rating_window_start, rating_window_end,
           -- SIGNED: the average of the present slopes, sign preserved. Falls
           -- are as much a historic datapoint as rises — this number is the
           -- durable per-snapshot momentum record, so it must not clamp
           -- downside.
           round((
               COALESCE(vibe_slope, 0) + COALESCE(rating_slope, 0)
           ) / NULLIF(
               (CASE WHEN vibe_slope IS NULL THEN 0 ELSE 1 END)
               + (CASE WHEN rating_slope IS NULL THEN 0 ELSE 1 END),
               0
           ), 3) AS momentum_score
    FROM enriched
    WHERE vibe_slope IS NOT NULL OR rating_slope IS NOT NULL;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;

    RETURN inserted_count;
END;
$$;


DROP FUNCTION public.momentum_week_window(text);
CREATE FUNCTION public.character_reporting_date() RETURNS date LANGUAGE sql STABLE AS $$
    SELECT COALESCE((e.cutoff_at AT TIME ZONE 'America/Detroit')::date-1,
        (current_timestamp AT TIME ZONE 'America/Detroit')::date)
    FROM public.coverage_schedule s LEFT JOIN public.coverage_editions e
        ON s.enabled AND s.phase='characters' AND e.edition_date=s.edition_date
$$;
CREATE TABLE public.character_daily_publications (
    reporting_date date NOT NULL,stage text NOT NULL,entity_type text NOT NULL,
    entity_id bigint NOT NULL,sport text NOT NULL,publication_txid bigint NOT NULL,
    created_at timestamptz NOT NULL DEFAULT clock_timestamp(),
    PRIMARY KEY(reporting_date,stage,entity_type,entity_id,sport)
);
CREATE FUNCTION public.guard_daily_character_publication() RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE task text; saved bigint;
BEGIN
    PERFORM public.coverage_lock();
    IF NOT (SELECT enabled FROM public.coverage_schedule) THEN RETURN NEW; END IF;
    task:=CASE TG_TABLE_NAME WHEN 'news_summaries' THEN 'narratives' WHEN 'vibe_scores' THEN 'vibe'
        WHEN 'stat_summaries' THEN 'rating' WHEN 'insider_scores' THEN 'transfers'
        WHEN 'momentum_summaries' THEN 'momentum' WHEN 'sigil_synthesis' THEN 'sigil' END;
    IF NOT EXISTS(SELECT 1 FROM public.pipeline_work w WHERE w.stage=task
        AND w.entity_type=NEW.entity_type AND w.entity_id=NEW.entity_id AND w.sport=NEW.sport
        AND w.status='running' AND w.claim_token::text=current_setting('scoracle.claim_token',true)) THEN
        RAISE EXCEPTION 'Daily character publication requires its current queue lease';
    END IF;
    IF NOT public.coverage_publishable(task,NEW.entity_type,NEW.entity_id,NEW.sport)
        OR NEW.reporting_date<>public.character_reporting_date() THEN
        RAISE EXCEPTION 'Character publication is outside its daily assignment';
    END IF;
    INSERT INTO public.character_daily_publications(reporting_date,stage,entity_type,entity_id,sport,publication_txid)
    VALUES(NEW.reporting_date,task,NEW.entity_type,NEW.entity_id,NEW.sport,txid_current())
    ON CONFLICT(reporting_date,stage,entity_type,entity_id,sport) DO UPDATE
        SET publication_txid=EXCLUDED.publication_txid
        WHERE character_daily_publications.publication_txid=EXCLUDED.publication_txid
    RETURNING publication_txid INTO saved;
    IF saved IS NULL THEN RAISE EXCEPTION 'Daily character output already published'; END IF;
    RETURN NEW;
END;
$$;
DO $$ DECLARE t text; BEGIN
    FOREACH t IN ARRAY ARRAY['news_summaries','vibe_scores','stat_summaries',
        'insider_scores','momentum_summaries','sigil_synthesis'] LOOP
        -- Alphabetic trigger order stamps the date before checking the publication boundary.
        EXECUTE format('CREATE TRIGGER zz_guard_daily_publication BEFORE INSERT ON public.%I
            FOR EACH ROW EXECUTE FUNCTION public.guard_daily_character_publication()',t);
    END LOOP;
END; $$;

INSERT INTO public.schema_migrations(version) VALUES ('310_daily_character_scope') ON CONFLICT DO NOTHING;
COMMIT;

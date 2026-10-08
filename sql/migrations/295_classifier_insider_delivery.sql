BEGIN;
-- Canonical names supply identity candidates, never move/roster conclusions.
CREATE OR REPLACE FUNCTION public.classifier_identity_candidates(league text, headline text, body text)
RETURNS jsonb LANGUAGE sql STABLE AS $$
    WITH unique_names AS (
        SELECT s.entity_type,s.entity_id,s.norm FROM public.entity_name_surfaces s
        WHERE s.sport=league AND s.surface_kind='name' AND s.norm<>''
        AND NOT EXISTS(SELECT 1 FROM public.entity_name_surfaces other
            WHERE other.sport=s.sport AND other.norm=s.norm
            AND (other.entity_type,other.entity_id)<>(s.entity_type,s.entity_id))
        AND strpos(' '||public.nrm(headline||' '||body)||' ',' '||s.norm||' ')>0
    ), candidates AS (
        SELECT DISTINCT n.entity_type,n.entity_id,COALESCE(t.name,p.name,pp.full_name) AS name
        FROM unique_names n
        LEFT JOIN public.teams t ON n.entity_type='team' AND t.id=n.entity_id AND t.sport=league
        LEFT JOIN public.players p ON n.entity_type='player' AND p.id=n.entity_id AND p.sport=league
        LEFT JOIN public.persons pp ON n.entity_type='person' AND pp.id=n.entity_id AND pp.sport=league AND pp.kind='coach'
        WHERE n.norm=public.nrm(COALESCE(t.name,p.name,pp.full_name))
    ) SELECT COALESCE(jsonb_agg(jsonb_build_object('entity_type',entity_type,'entity_id',entity_id,
        'sport',league,'name',name,'identity_method','unique_canonical_name_v1')
        ORDER BY entity_type,entity_id) FILTER(WHERE name IS NOT NULL),'[]'::jsonb) FROM candidates
$$;
ALTER TABLE public.classifier_sources ADD COLUMN IF NOT EXISTS identity_hash text NOT NULL DEFAULT '';
ALTER TABLE public.classifier_sources DROP CONSTRAINT IF EXISTS classifier_sources_article_id_sport_input_hash_body_sha256_key;
CREATE UNIQUE INDEX IF NOT EXISTS classifier_source_identity ON public.classifier_sources(article_id,sport,input_hash,body_sha256,identity_hash);
CREATE OR REPLACE FUNCTION public.classifier_discovery_version(article bigint, league text)
RETURNS text LANGUAGE sql STABLE AS $$
    SELECT md5(jsonb_build_array(a.url,a.title,COALESCE(a.source,''),
        extract(epoch FROM a.published_at),COALESCE(a.full_text,''),a.duplicate_of,
        public.classifier_identity_candidates(league,a.title,COALESCE(NULLIF(a.full_text,''),
            (SELECT s.source->>'body' FROM public.classifier_sources s
             WHERE s.article_id=a.id AND s.sport=league AND s.source->>'url'=a.url ORDER BY s.id DESC LIMIT 1),'')),
        jsonb_agg(jsonb_build_array(p.entity_type,p.entity_id,p.sport,p.feed_rank,t.name)
            ORDER BY p.entity_type,p.entity_id))::text)
    FROM public.news_articles a
    JOIN public.harvester_query_provenance p ON p.article_id=a.id AND p.sport=league
    LEFT JOIN public.teams t ON p.entity_type='team' AND t.id=p.entity_id AND t.sport=p.sport
    WHERE a.id=article
    GROUP BY a.id
$$;

CREATE OR REPLACE FUNCTION public.classifier_dispatch_delivery()
RETURNS trigger LANGUAGE plpgsql AS $$
DECLARE
    target_type text;
    target_id integer;
    league text;
    revision text;
    destination text;
BEGIN
    IF NEW.status <> 'pending' OR NOT NEW.production_eligible THEN RETURN NEW; END IF;
    IF TG_OP = 'UPDATE' THEN
        IF OLD.status = 'pending' AND OLD.production_eligible
            AND OLD.policy_version = NEW.policy_version THEN RETURN NEW; END IF;
    END IF;
    destination := CASE NEW.plugin_id
        WHEN 'scoracle.character.narrative' THEN 'narratives'
        WHEN 'scoracle.character.vibe' THEN 'vibe'
        WHEN 'scoracle.character.transfers' THEN 'transfers' END;
    IF destination IS NULL THEN
        RAISE EXCEPTION 'Classifier delivery plugin is not wired: %', NEW.plugin_id;
    END IF;
    SELECT m.receipt->'target'->>'entity_type', (m.receipt->'target'->>'entity_id')::integer, m.sport
      INTO target_type,target_id,league FROM public.classifier_measurements m
      WHERE m.id=NEW.measurement_id AND m.status='source_bound_provisional';
    IF target_type IS NULL OR (target_type NOT IN ('team','player') AND NOT (destination='transfers' AND target_type='person')) OR target_id IS NULL OR target_id <= 0 THEN
        RAISE EXCEPTION 'Classifier delivery requires a canonical team/player target';
    END IF;
    revision := 'classifier-qualified-claims-v1:' || NEW.policy_version || ':m' || NEW.measurement_id;
    INSERT INTO public.pipeline_work(stage,entity_type,entity_id,sport,input_version)
    VALUES(destination,target_type,target_id,league,revision)
    ON CONFLICT(stage,entity_type,entity_id,sport) DO UPDATE SET
        status='pending',attempts=0,input_version=EXCLUDED.input_version,
        available_at=CASE WHEN pipeline_work.status='pending' THEN pipeline_work.available_at ELSE now() END,
        updated_at=now(),last_error=NULL,running_input_version=NULL,claim_token=NULL
    WHERE pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version;
    RETURN NEW;
END;
$$;
ALTER TABLE public.pipeline_work DROP CONSTRAINT IF EXISTS pipeline_work_entity_type_check;
ALTER TABLE public.pipeline_work ADD CONSTRAINT pipeline_work_entity_type_check
    CHECK(entity_type IN ('player','team','article','fixture','candidate','person'));
DO $$ BEGIN
    IF to_regclass('public.transfer_rumors') IS NOT NULL THEN
        ALTER TABLE public.transfer_rumors DROP CONSTRAINT IF EXISTS transfer_rumors_trigger_type_check;
        ALTER TABLE public.transfer_rumors ADD CONSTRAINT transfer_rumors_trigger_type_check
            CHECK(trigger_type IN ('news_spike','periodic','manual','harvester','classifier'));
    END IF;
END $$;
INSERT INTO public.schema_migrations(version) VALUES ('295_classifier_insider_delivery') ON CONFLICT DO NOTHING;
COMMIT;

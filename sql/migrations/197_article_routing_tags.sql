-- 197_article_routing_tags.sql
--
-- WHAT: `news_articles.routing_tags text[]`, a subscription table mapping a tag to the stages that
-- want it, and a fan-out trigger over newly-added tags.
--
-- WHY: `news_articles.bucket` holds ONE label, and mig 175 routes `bucket = 'transfer'` to the
-- Insider. A story that is both a transfer and emotionally charged -- Diomande choosing Real Madrid
-- over PSG -- cannot be expressed by it, so it can only ever reach one voice. That single-valued
-- column is the thing standing between the Editor and a newsroom: three peers reading ONE packet
-- through three lenses is what lets them tell different parts of the same story, and disagree.
--
-- Tags are CONTENT FACTS, not voice names. `transfer`, `injury`, `roster` describe the article;
-- `stage_routing_subscriptions` says who cares. That split is deliberate -- it keeps E1's routing
-- decision as DATA (an INSERT) rather than as code, and it means adding the Influencer later never
-- touches this trigger.
--
-- Tags are DERIVED, never asked. They project from `story_type`, which the Editor already emits,
-- and later from the emotional register (C2). Asking a 4B "which voices should see this?" is a
-- judgment call, and this codebase has paid three prompt revisions (ar3/ar5/ar6) learning that it
-- will not render those reliably.
--
-- ## Inert on arrival, deliberately
--
-- `stage_routing_subscriptions` ships EMPTY, so the trigger fans out to nobody and no stage changes
-- behaviour. Transfers keeps running off mig 175 until Phase E migrates it deliberately -- seeding
-- ('transfer','transfers') here would double-enqueue against mig 175 with a DIFFERENT
-- input_version, which is not a duplicate (ON CONFLICT handles that) but a churn loop: the two
-- fingerprints would alternate and reopen the item forever. The slowest stage in the pipeline is
-- not where you discover that.
--
-- ## The re-wake guard is the whole design
--
-- Only tags in NEW and NOT in OLD fan out. A packet that gains an article re-fans exactly to the
-- voices whose slice actually changed -- the Scout wakes on a new `injury` tag, the Insider does
-- not wake again for a `transfer` tag it already saw. This is mig 175's `IS DISTINCT FROM` guard
-- generalized: without it, every article on a running saga would wake every subscribed voice for
-- every involved entity.
--
-- Deploy order: additive. The column defaults to '{}' and nothing writes it until the binary ships.

BEGIN;

-- ---------------------------------------------------------------------------
-- 1. The tags themselves
-- ---------------------------------------------------------------------------

ALTER TABLE public.news_articles
    ADD COLUMN IF NOT EXISTS routing_tags text[] NOT NULL DEFAULT '{}';

COMMENT ON COLUMN public.news_articles.routing_tags IS
    'Content facts derived from the Editor''s story_type (and later its emotional register) -- '
    '"transfer", "injury", "roster", ... An article carries ALL that apply, which is what lets one '
    'story reach several voices at once. Superset of `bucket`, which is single-valued and stays '
    'until Phase E retires it. Tags are DERIVED in code, never asked of the model.';

CREATE INDEX IF NOT EXISTS idx_news_articles_routing_tags
    ON public.news_articles USING GIN (routing_tags);

-- ---------------------------------------------------------------------------
-- 2. Who subscribes to what
-- ---------------------------------------------------------------------------

CREATE TABLE IF NOT EXISTS public.stage_routing_subscriptions (
    tag          text NOT NULL,
    stage        text NOT NULL,
    entity_type  text NOT NULL,
    note         text,
    created_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (tag, stage, entity_type)
);

COMMENT ON TABLE public.stage_routing_subscriptions IS
    'Maps a news_articles.routing_tags value to the stage that wants it, at the entity grain that '
    'stage is keyed on. Ships EMPTY: transfers still routes via mig 175, and Phase E migrates it '
    'with one INSERT per voice. Keeping this as DATA is the point -- adding the Influencer to the '
    'newsroom should be a row, not a trigger rewrite.';

-- ---------------------------------------------------------------------------
-- 3. The fan-out
-- ---------------------------------------------------------------------------

CREATE OR REPLACE FUNCTION public.enqueue_voices_on_routing_tags() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- One work item per (subscribing stage, entity of the matching grain), for tags that were
    -- ADDED by this update. `input_version` is that entity's fingerprint over its canonical,
    -- vetted, in-window articles CARRYING THE SAME TAG -- the direct generalization of mig 175's
    -- transfer-corpus fingerprint. ON CONFLICT therefore reopens an item exactly when that voice's
    -- slice moved and debounces one that did not.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT s.stage, s.entity_type, te.entity_id, te.sport, 'pending',
           's:' || t.tag || ':' || (
               SELECT count(*) || ':' ||
                      COALESCE(md5(string_agg(x.article_id::text, ',' ORDER BY x.article_id)), '0')
                 FROM public.news_article_entities x
                 JOIN public.news_articles xa ON xa.id = x.article_id
                WHERE x.entity_type = s.entity_type
                  AND x.entity_id   = te.entity_id
                  AND x.sport       = te.sport
                  AND x.vetted IS TRUE
                  AND xa.duplicate_of IS NULL
                  AND xa.routing_tags @> ARRAY[t.tag]
                  AND x.created_at > NOW() - INTERVAL '14 days'
           ),
           NOW(), NOW()
      FROM (
            SELECT unnest(NEW.routing_tags)
            EXCEPT
            SELECT unnest(COALESCE(OLD.routing_tags, '{}'))
           ) AS t(tag)
      JOIN public.stage_routing_subscriptions s ON s.tag = t.tag
      JOIN public.news_article_entities te
        ON te.article_id  = NEW.id
       AND te.entity_type = s.entity_type
       AND te.vetted IS TRUE
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    -- Statement-level mig-133 notify trigger on pipeline_work covers the wake-up; no PERFORM here.
    RETURN NEW;
END;
$$;

DROP TRIGGER IF EXISTS enqueue_voices_on_routing_tags ON public.news_articles;
CREATE TRIGGER enqueue_voices_on_routing_tags
    AFTER UPDATE OF routing_tags ON public.news_articles
    FOR EACH ROW
    WHEN (NEW.routing_tags IS DISTINCT FROM OLD.routing_tags)
    EXECUTE FUNCTION public.enqueue_voices_on_routing_tags();

INSERT INTO public.schema_migrations(version) VALUES ('197_article_routing_tags')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

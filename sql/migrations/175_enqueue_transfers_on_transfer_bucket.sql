-- 175_enqueue_transfers_on_transfer_bucket.sql
--
-- Cognition refactor, Phase 3 (DAG re-sequence). transfers is now gated by the Journalist's
-- per-article transfer label. When narratives (n9) flips `news_articles.bucket` to 'transfer',
-- enqueue the transfers stage for that article's vetted TEAM entities — the grain the transfers
-- stage is keyed on (stage='transfers', entity_type='team'). This replaces the blanket
-- "every vetted team → transfers each cycle" fan-out that mig 174 removed from
-- enqueue_derive_on_vetted: transfers now runs only for articles a curated voice judged
-- transfer-related, and only for the teams that article actually names.
--
-- The narratives persist writes bucket with `... WHERE bucket IS DISTINCT FROM v.bucket`, and this
-- trigger's WHEN clause requires an actual flip TO 'transfer', so an already-'transfer' article
-- re-labeled the same way never re-fires — no per-cycle re-enqueue churn.
--
-- SHIPS TOGETHER with mig 174 and the n9 binary (nothing writes news_articles.bucket until the n9
-- narratives handler lands, so this trigger is inert until then). The transfers handler's corpus
-- filter (`bucket IS DISTINCT FROM 'non_transfer'`) is unchanged — it already reads the label this
-- writes. Deploy order per mig 174's header (F-022: binary first, then 174 + 175 together).

BEGIN;

CREATE OR REPLACE FUNCTION public.enqueue_transfers_if_transfer_related() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
BEGIN
    -- One transfers item per vetted TEAM entity of the newly-transfer-bucketed article. The
    -- input_version is the team's CANONICAL transfer-corpus fingerprint — vetted, duplicate_of IS
    -- NULL, transfer bucket (`IS DISTINCT FROM 'non_transfer'`, matching the transfers handler),
    -- inside the handler's 14-day link window — so ON CONFLICT reopens the item exactly when that
    -- corpus moved and debounces an unchanged one. COALESCE(...,'0') keeps the key non-null when a
    -- team's set is momentarily empty (defensive; the driving article normally makes it non-empty).
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    SELECT 'transfers', 'team', te.entity_id, te.sport, 'pending',
           't:' || (
               SELECT count(*) || ':' ||
                      COALESCE(md5(string_agg(x.article_id::text, ',' ORDER BY x.article_id)), '0')
                 FROM public.news_article_entities x
                 JOIN public.news_articles xa ON xa.id = x.article_id
                WHERE x.entity_type = 'team'
                  AND x.entity_id   = te.entity_id
                  AND x.sport       = te.sport
                  AND x.vetted IS TRUE
                  AND xa.duplicate_of IS NULL
                  AND xa.bucket IS DISTINCT FROM 'non_transfer'
                  AND x.created_at > NOW() - INTERVAL '14 days'
           ),
           NOW(), NOW()
      FROM public.news_article_entities te
     WHERE te.article_id   = NEW.id
       AND te.entity_type   = 'team'
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

DROP TRIGGER IF EXISTS enqueue_transfers_if_transfer_related ON public.news_articles;
CREATE TRIGGER enqueue_transfers_if_transfer_related
    AFTER UPDATE OF bucket ON public.news_articles
    FOR EACH ROW
    WHEN (NEW.bucket = 'transfer' AND NEW.bucket IS DISTINCT FROM OLD.bucket)
    EXECUTE FUNCTION public.enqueue_transfers_if_transfer_related();

INSERT INTO public.schema_migrations(version) VALUES ('175_enqueue_transfers_on_transfer_bucket')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

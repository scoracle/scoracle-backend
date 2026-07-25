BEGIN;

-- 193_graph_after_article_read.sql
--
-- Mig 188 enqueued `graph` and `article_read` together on the same vetted write, both with
-- available_at = NOW(). Handler registration order is scrub(1) / graph(2) / article_read(3)
-- (rust/src/main.rs:100/106/109), so graph always won the race: it PROVABLY never saw a summary.
-- Every graph extraction to date has run on headline + RSS blurb while the body sat one stage away.
--
-- Worse, graph ran on articles that were already dead: `load_graph_article_context` has no
-- duplicate filter, and the Article Reader's own relevance verdict (mig 190, `irrelevant`, which
-- overturns the scrub gate on 12.1% of what it reads) lands strictly after graph has already spent
-- its model call. Measured 2026-07-25: graph is 45.8 calls/hr — the single largest article-plumbing
-- consumer on a GPU that is oversubscribed several times over.
--
-- After this migration the vetted trigger enqueues ONLY `article_read`, and the Rust
-- ArticleReadHandler enqueues `graph` from its success path. That makes the ordering a data
-- dependency instead of a lucky registration order, and it means graph is never enqueued at all for
-- an article that turned out to be a duplicate, unreadable, or irrelevant.
--
-- This is a quality fix and a capacity fix at once: graph sees `evidence_blurb` for the first time,
-- and stops paying for articles nothing downstream will ever read.

CREATE OR REPLACE FUNCTION public.enqueue_derive_on_vetted() RETURNS trigger
    LANGUAGE plpgsql
    AS $$
DECLARE
    v_vetted_count integer;
BEGIN
    SELECT count(*) INTO v_vetted_count
      FROM public.news_article_entities
     WHERE article_id = NEW.article_id AND vetted IS TRUE;

    -- Article Reader is now the ONLY stage enqueued on a vetted write. It fetches the body,
    -- summarizes, validates co-mentions, and may reject the article outright; whatever survives
    -- that, it enqueues onward itself. `graph` is deliberately absent here — see the header.
    INSERT INTO public.pipeline_work
        (stage, entity_type, entity_id, sport, status, input_version, available_at, updated_at)
    VALUES ('article_read', 'article', NEW.article_id::integer, NEW.sport, 'pending',
            'ar:' || v_vetted_count, NOW(), NOW())
    ON CONFLICT (stage, entity_type, entity_id, sport) DO UPDATE SET
        status        = 'pending',
        attempts      = 0,
        available_at  = NOW(),
        updated_at    = NOW(),
        last_error    = NULL,
        input_version = EXCLUDED.input_version
    WHERE public.pipeline_work.input_version IS DISTINCT FROM EXCLUDED.input_version
       OR public.pipeline_work.status = 'failed';

    PERFORM pg_notify('pipeline_work_ready', '');
    RETURN NEW;
END;
$$;

COMMENT ON FUNCTION public.enqueue_derive_on_vetted() IS
    'Enqueues article_read on a vetted news link. graph is NOT enqueued here — it is enqueued by '
    'the Rust ArticleReadHandler on its success path (mig 193) so it reads the summary rather than '
    'racing the reader for the headline.';

-- Drain the graph work that mig 188 queued but that can no longer see a summary. These rows would
-- otherwise burn GPU on exactly the articles this migration exists to stop paying for: ones already
-- suppressed as duplicates, or whose reader verdict has not run yet and will re-enqueue properly.
DELETE FROM public.pipeline_work pw
 WHERE pw.stage = 'graph'
   AND pw.status = 'pending'
   AND (
        EXISTS (SELECT 1 FROM public.news_articles a
                 WHERE a.id = pw.entity_id AND a.duplicate_of IS NOT NULL)
     OR NOT EXISTS (SELECT 1 FROM public.news_article_readings r
                     WHERE r.article_id = pw.entity_id AND r.status = 'success')
   );

INSERT INTO public.schema_migrations(version)
VALUES ('193_graph_after_article_read')
ON CONFLICT DO NOTHING;

COMMIT;

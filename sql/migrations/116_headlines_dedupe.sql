-- 116_headlines_dedupe.sql
--
-- Headlines v1 initially appended every model-emitted row. Because each run scans
-- the recent corpus, the same source article/title can be emitted repeatedly
-- across reruns and sometimes more than once in a single model response. Keep the
-- newest row for each concrete duplicate and add unique indexes as a durable
-- backstop.

BEGIN;

WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY sport, entity_type, entity_id, lower(btrim(source_url))
               ORDER BY generated_at DESC NULLS LAST, published_at DESC, id DESC
           ) AS rn
    FROM public.headlines
    WHERE NULLIF(btrim(source_url), '') IS NOT NULL
)
DELETE FROM public.headlines h
USING ranked r
WHERE h.id = r.id
  AND r.rn > 1;

WITH ranked AS (
    SELECT id,
           row_number() OVER (
               PARTITION BY sport, entity_type, entity_id,
                            lower(btrim(source_name)), lower(btrim(title))
               ORDER BY generated_at DESC NULLS LAST, published_at DESC, id DESC
           ) AS rn
    FROM public.headlines
    WHERE NULLIF(btrim(source_name), '') IS NOT NULL
)
DELETE FROM public.headlines h
USING ranked r
WHERE h.id = r.id
  AND r.rn > 1;

CREATE UNIQUE INDEX IF NOT EXISTS idx_headlines_entity_source_url_uniq
    ON public.headlines (sport, entity_type, entity_id, lower(btrim(source_url)))
    WHERE NULLIF(btrim(source_url), '') IS NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS idx_headlines_entity_source_title_uniq
    ON public.headlines (sport, entity_type, entity_id, lower(btrim(source_name)), lower(btrim(title)))
    WHERE NULLIF(btrim(source_name), '') IS NOT NULL;

COMMIT;

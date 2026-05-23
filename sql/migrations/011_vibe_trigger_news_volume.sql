-- News-volume vibe trigger.
--
-- When an entity (player or team) accumulates 5 distinct news articles
-- inside a 60-minute rolling window, fire pg_notify('vibe_trigger', ...)
-- so the Go listener can run Gemma against the entity. The crossing
-- semantic (count == 4 immediately before this insert, so this insert
-- makes 5) ensures NOTIFY fires exactly once per spike — even if 50
-- articles arrive in a few minutes, we get one notification, not 46.
--
-- Cron-driven corpus mode also writes to news_article_entities, so this
-- trigger fires there too. That's intentional: a noon corpus run that
-- ingests a breaking-news cycle for an offseason entity will trigger an
-- immediate vibe refresh rather than waiting another 12h. The Go-side
-- 30-min debounce prevents double-scoring within a single corpus run.

CREATE OR REPLACE FUNCTION notify_vibe_trigger() RETURNS TRIGGER AS $$
DECLARE
    prior_count INTEGER;
BEGIN
    SELECT COUNT(DISTINCT article_id) INTO prior_count
    FROM news_article_entities
    WHERE entity_type = NEW.entity_type
      AND entity_id = NEW.entity_id
      AND sport = NEW.sport
      AND created_at > NOW() - INTERVAL '60 minutes'
      AND article_id <> NEW.article_id;

    IF prior_count = 4 THEN
        PERFORM pg_notify('vibe_trigger', json_build_object(
            'entity_type', NEW.entity_type,
            'entity_id', NEW.entity_id,
            'sport', NEW.sport,
            'article_count', prior_count + 1,
            'ts', extract(epoch from now())::bigint
        )::text);
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

DROP TRIGGER IF EXISTS trg_vibe_trigger_on_news_link ON news_article_entities;

CREATE TRIGGER trg_vibe_trigger_on_news_link
    AFTER INSERT ON news_article_entities
    FOR EACH ROW
    EXECUTE FUNCTION notify_vibe_trigger();

-- Index covering the trigger's lookup. Without this, every insert pays a
-- seq scan over news_article_entities. The existing
-- idx_news_entities_lookup is on (entity_type, entity_id, sport) but
-- doesn't include created_at, so the trigger's time-window filter would
-- still hit the heap.
CREATE INDEX IF NOT EXISTS idx_news_entities_lookup_created
    ON news_article_entities(entity_type, entity_id, sport, created_at);

-- 034_transfer_trigger.sql
--
-- Real-time transfer refresh. Reuse the EXISTING news-volume crossing
-- (notify_vibe_trigger, migration 011): when a TEAM accumulates its 5th distinct
-- news article inside a 60-minute window, also fire pg_notify('transfer_trigger',
-- ...) so the Go listener (internal/listener/transfer_worker.go) can re-vet that
-- team's rumor candidates within minutes instead of waiting for the next cron.
--
-- Why extend the one function rather than add a second trigger: both signals key
-- off the identical 4→5 crossing and the same covering index
-- (idx_news_entities_lookup_created). A separate trigger would re-run the same
-- COUNT(DISTINCT ...) on every insert — double the per-insert cost for no new
-- information. One function, one count, two notifies.
--
-- TEAM-only: the transfer card is per-team, and GenerateForTeam needs a team. A
-- player news spike is usually match coverage, not a transfer, and isn't directly
-- actionable for the team card in v1 — so we don't put it on the channel. (The
-- vibe notify still fires for both entity types, unchanged.) The Go listener also
-- defensively ignores non-team events.

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

        -- Same crossing, transfer channel — teams only (the card's grain).
        IF NEW.entity_type = 'team' THEN
            PERFORM pg_notify('transfer_trigger', json_build_object(
                'entity_type', NEW.entity_type,
                'entity_id', NEW.entity_id,
                'sport', NEW.sport,
                'article_count', prior_count + 1,
                'ts', extract(epoch from now())::bigint
            )::text);
        END IF;
    END IF;

    RETURN NEW;
END;
$$ LANGUAGE plpgsql;

-- Trigger + covering index unchanged (migration 011); CREATE OR REPLACE above is
-- sufficient — the AFTER INSERT trigger already points at this function.

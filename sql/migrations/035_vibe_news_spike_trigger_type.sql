-- 035_vibe_news_spike_trigger_type.sql
--
-- Bug fix: vibe_scores.trigger_type CHECK (migration 007) never allowed
-- 'news_spike', but the news-volume LISTEN/NOTIFY worker
-- (internal/listener/news_volume_worker.go) has always written exactly that on a
-- spike. Every spike-driven vibe generation therefore failed at persist with
-- "violates check constraint vibe_scores_trigger_type_check" — i.e. real-time vibe
-- scoring has silently never persisted; only the twice-daily 'periodic' cron and
-- 'milestone'/'manual' paths landed rows.
--
-- (transfer_rumors got this right from day one — its CHECK includes 'news_spike',
-- migration 031. This aligns vibe_scores with that.)
--
-- Widen the CHECK to the full set the code emits: 'milestone' | 'manual' |
-- 'periodic' | 'news_spike'.

BEGIN;

ALTER TABLE vibe_scores DROP CONSTRAINT IF EXISTS vibe_scores_trigger_type_check;

ALTER TABLE vibe_scores ADD CONSTRAINT vibe_scores_trigger_type_check
    CHECK (trigger_type IN ('milestone', 'manual', 'periodic', 'news_spike'));

COMMIT;

-- 151_topic_heat_embeddings.sql
--
-- Persistent write-through cache for topic-heat article embeddings (2026-07-16
-- follow-up to the worker-loop hardening deploy: the cold in-memory cache made the
-- startup tick embed 933 articles on CPU for 27 minutes before the first work claim,
-- and a hot corpus past ~1,500 articles would push that pass toward the 2700s
-- watchdog threshold — a boot-loop).
--
-- The worker's in-memory TopicHeatCache (rust/src/bucket.rs) hydrates from this
-- table once per process, so a restart re-embeds only articles that arrived or were
-- edited while the process was down. Pure cache semantics:
--   * embedding is little-endian f32 bytes, L2-normalized (bge-small = 384 dims =
--     1536 bytes); clustering stays in Rust and nothing queries by similarity, so
--     plain bytea — no pgvector dependency.
--   * `fingerprint` is the worker's text_fingerprint (u64 stored as i64, same bit
--     pattern); a row is served only when the fingerprint matches the current text.
--   * `model` is the embedder identity (repo@revision|pooling|max_tokens) — a model
--     swap invalidates every row without needing a migration.
--   * rows are pruned to the current lookback window at the end of every refresh
--     pass (mirror of TopicHeatCache::retain_ids), and follow article deletion via
--     ON DELETE CASCADE. Worst-case staleness is reproducible from news_articles.
--
-- NOT applied at author time — production DB migration requires named-user approval.
-- Sequencing: the binary tolerates the table's absence (hydration failure degrades
-- to the old cold-pass behavior with a WARN), but apply-before-deploy is preferred
-- so the first boot after deploy is already warm-capable.

BEGIN;

CREATE TABLE public.topic_heat_embeddings (
    article_id  bigint PRIMARY KEY
                REFERENCES public.news_articles(id) ON DELETE CASCADE,
    fingerprint bigint      NOT NULL,
    model       text        NOT NULL,
    embedding   bytea       NOT NULL,
    embedded_at timestamptz NOT NULL DEFAULT now()
);

COMMENT ON TABLE public.topic_heat_embeddings IS
    'Write-through cache of topic-heat article embeddings (mig 151). Reproducible from news_articles text; safe to truncate.';

INSERT INTO public.schema_migrations(version) VALUES ('151_topic_heat_embeddings')
    ON CONFLICT DO NOTHING;

COMMIT;

-- 137_source_tiers_news_only.sql
--
-- C4 (DATA_FLOW_FRICTION_PRUNE_PLAN, Wave 2): remove the dead twitter source tiers.
--
-- 098_decommission_tweets dropped the tweets + tweet_entities tables, but the 7
-- kind='twitter' seed rows (Romano, Ornstein, Woj, Shams, Schefter, RapSheet,
-- TheAthleticFC) survived, along with the half-dead CHECK arm
-- kind IN ('news','twitter'). The live compute_transfer_heat (mig-104) hardcodes
-- 'news'::text AS kind in its corpus CTE, so the twitter rows are never matched;
-- transfer.rs:368 loads the whole table into the tier map but only ever looks up
-- news:* keys, so the twitter rows ride along as dead map entries. Deleting them and
-- tightening the CHECK to kind='news' makes the table reflect what the flow queries.

BEGIN;

DELETE FROM public.source_tiers WHERE kind = 'twitter';

ALTER TABLE public.source_tiers DROP CONSTRAINT source_tiers_kind_check;
ALTER TABLE public.source_tiers ADD CONSTRAINT source_tiers_kind_check CHECK (kind = 'news');

INSERT INTO public.schema_migrations(version) VALUES ('137_source_tiers_news_only')
    ON CONFLICT DO NOTHING;

COMMIT;

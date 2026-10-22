#!/usr/bin/env bash
# Read-only deployment/schema contract check. Does not enable workers or policies.
set -euo pipefail
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
if [ -z "${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}" ] && [ -f "$ROOT/.env.local" ]; then
    set -a; source "$ROOT/.env.local"; set +a
fi
DB="${1:-${DATABASE_PRIVATE_URL:-${DATABASE_URL:-}}}"
[ -n "$DB" ] || { echo "verify-classifier: database URL required" >&2; exit 1; }
psql "$DB" -X -q -v ON_ERROR_STOP=1 <<'SQL'
BEGIN READ ONLY;
DO $$
DECLARE migration_version text;
BEGIN
    FOREACH migration_version IN ARRAY ARRAY['291_classifier_plumbing','292_classifier_acquisition_intake',
        '293_classifier_character_delivery','294_classifier_influencer_delivery',
        '295_classifier_insider_delivery','296_classifier_scout_delivery',
        '297_classifier_journalist_product','298_classifier_article_identities',
        '299_classifier_spectra','300_classifier_spectrum_deliveries','301_classifier_spectrum_dispatch',
        '302_classifier_source_revisions','303_classifier_spectrum_consumption',
        '304_classifier_article_spectrum','305_classifier_identity_prefill',
        '306_pipeline_stage_progress','307_classifier_bounded_acquisition','308_retire_graph','309_continuous_coverage','310_daily_character_scope','311_daily_pipeline_recovery','312_classifier_subject_reader','313_classifier_headline_first','314_classifier_headline_corpus','315_headline_entity_pipeline',
        '318_coverage_freeze_set_based','319_nba_free_throw_measure',
        '320_scout_availability_and_bounded_reconciliation','321_durable_qualified_editions'] LOOP
        IF NOT EXISTS(SELECT 1 FROM public.schema_migrations m WHERE m.version=migration_version) THEN
            RAISE EXCEPTION 'Missing migration %; migrate before placing binaries',migration_version;
        END IF;
    END LOOP;
    IF to_regprocedure('public.classifier_enqueue_admission(bigint,text)') IS NULL
       OR to_regprocedure('public.classifier_replay_admission(integer)') IS NULL
       OR to_regprocedure('public.classifier_headline_subjects(bigint,text)') IS NULL
       OR to_regprocedure('public.classifier_identity_candidates(text,text,text)') IS NULL THEN
        RAISE EXCEPTION 'Classifier headline intake functions missing';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.classifier_spectrum_deliveries'::regclass
        AND tgname='classifier_spectrum_delivery_dispatch' AND NOT tgisinternal AND tgenabled IN ('O','A')) THEN
        RAISE EXCEPTION 'Classifier delivery dispatch trigger missing or disabled';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.pipeline_work'::regclass
        AND tgname='pipeline_record_completion' AND NOT tgisinternal AND tgenabled IN ('O','A')) THEN
        RAISE EXCEPTION 'Pipeline successful-stage completion trigger missing or disabled';
    END IF;
END $$;
DO $$ BEGIN
    IF NOT EXISTS(SELECT 1 FROM pg_constraint WHERE conrelid='public.pipeline_work'::regclass
        AND conname='pipeline_work_graph_retired' AND convalidated) THEN
        RAISE EXCEPTION 'Graph retirement queue constraint missing';
    END IF;
END $$;
DO $$ BEGIN
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.classifier_spectrum_deliveries'::regclass
        AND tgname='classifier_article_ready' AND NOT tgisinternal AND tgenabled IN ('O','A')) THEN
        RAISE EXCEPTION 'Headline evidence readiness trigger missing or disabled';
    END IF;
END $$;
SELECT classifier_source_id FROM public.news_article_entities LIMIT 0;
SELECT source_id FROM public.classifier_source_heads LIMIT 0;
SELECT request_hash,receipt FROM public.classifier_spectra LIMIT 0;
SELECT stage,completed_at FROM public.pipeline_stage_progress LIMIT 0;
SELECT enabled,phase,collection_cutoff,timezone,last_tick_at FROM public.coverage_schedule;
SELECT edition_date,revision FROM public.coverage_publication;
SELECT candidate_edition FROM public.qualified_vibe_scores LIMIT 0;
SELECT resolution FROM public.coverage_assignments LIMIT 0;
SELECT public.coverage_collection_open(),public.coverage_collection_deadline();
SELECT public.character_reporting_date();
SELECT reporting_date,stage FROM public.character_daily_publications LIMIT 0;
SELECT prepared_input FROM public.coverage_assignments LIMIT 0;
SELECT edition_date FROM public.application_outbox LIMIT 0;
SELECT public.coverage_claimable('classifier','article',0,'NBA'),
    public.coverage_publishable('rating','team',0,'NBA');
SELECT status,count(*) AS deliveries FROM public.classifier_spectrum_deliveries GROUP BY status ORDER BY status;
SELECT stage,status,count(*) AS work FROM public.pipeline_work
    WHERE stage IN ('classifier_acquire','classifier','graph','harvester','editor')
    GROUP BY stage,status ORDER BY stage,status;
COMMIT;
SQL
printf 'Classifier schema ready; model qualification and policy release are separate.\n'

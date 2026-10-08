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
        '297_classifier_journalist_product','298_classifier_article_identities'] LOOP
        IF NOT EXISTS(SELECT 1 FROM public.schema_migrations m WHERE m.version=migration_version) THEN
            RAISE EXCEPTION 'Missing migration %; migrate before placing binaries',migration_version;
        END IF;
    END LOOP;
    IF to_regprocedure('public.classifier_enqueue_acquisition(bigint,text)') IS NULL
       OR to_regprocedure('public.classifier_replay_acquisition(integer)') IS NULL
       OR to_regprocedure('public.classifier_identity_candidates(text,text,text)') IS NULL THEN
        RAISE EXCEPTION 'Classifier native acquisition functions missing';
    END IF;
    IF NOT EXISTS(SELECT 1 FROM pg_trigger WHERE tgrelid='public.classifier_deliveries'::regclass
        AND tgname='classifier_delivery_dispatch' AND NOT tgisinternal AND tgenabled IN ('O','A')) THEN
        RAISE EXCEPTION 'Classifier delivery dispatch trigger missing or disabled';
    END IF;
END $$;
SELECT classifier_source_id FROM public.news_article_entities LIMIT 0;
SELECT status,count(*) AS deliveries FROM public.classifier_deliveries GROUP BY status ORDER BY status;
SELECT stage,status,count(*) AS work FROM public.pipeline_work
    WHERE stage IN ('classifier_acquire','classifier','graph','harvester','editor')
    GROUP BY stage,status ORDER BY stage,status;
COMMIT;
SQL
printf 'Classifier schema ready; model qualification and policy release are separate.\n'

-- 215_schema_audit_metadata_and_drivers.sql
--
-- D-T22, the schema audit, executed. Two things, both settled by measurement:
--
--   (A) THE STALLED METADATA FEATURE. `trg_detect_team_change` (LIVE, enabled, on
--       event_box_scores) has been maintaining `player_team_history` AND enqueuing into
--       `metadata_refresh_queue` since 2026-04-18. The consumer side exists only as SQL
--       (get_metadata_queue_batch / get_metadata_queue_status / mark_metadata_processed
--       and the metadata_queue_status view); NOTHING in Rust, Go, Python or a shell
--       heredoc has ever called any of it. Result: 42,782 rows queued, processed_at NULL
--       on every one, 0 ever drained. The queue stopped growing on 2026-06-17 only
--       because box scores stopped arriving (off-season) — the producer is armed and
--       WILL resume writing into a drain that does not exist when fixtures return.
--
--       SCOTT'S CALL 2026-08-06: keep the history, drop the queue. `player_team_history`
--       is a genuinely useful player->team record (valid_from/valid_until/is_current) and
--       stays, still maintained by the trigger. The refresh queue, its never-written
--       completion log, its view and its three orphan consumer functions go. This
--       migration NARROWS detect_team_change (it stops enqueuing, keeps writing history)
--       in the SAME transaction that drops the queue, so the trigger is never pointing at
--       a missing table — dropping the tables while the trigger still enqueued would break
--       every event_box_scores INSERT at season start, which is the worst possible time to
--       find out.
--
--       detect_team_change is derived from the CURRENT prod definition
--       (pg_get_functiondef, 2026-08-06 18:45) per the template rule, not from
--       sql/metadata_system.sql, which is stale and applied by nothing.
--
--   (B) THE SQL-ONLY CLASS — the audit's most useful result, and the reason this
--       migration is mostly comments. These tables' entire read/write lifecycle happens
--       inside the database, or inside a cron'd psql heredoc, or inside the Python seed
--       layer. Every one of them is a place where the next person greps the Rust and Go,
--       finds nothing, and concludes the table is dead. `compute_transfer_heat` already
--       cost us six hours that way. The fix is a COMMENT, not a migration — so each table
--       below now NAMES ITS DRIVER in its own COMMENT ON TABLE.
--
-- Deploy order: this migration is a DROP of objects with no application caller (proved
-- below) plus comments; no binary depends on the dropped names, so no release ordering
-- applies. F-022 does not bite here.
--
-- PROOF FOR THE DROPS (run 2026-08-06 18:44 EDT against prod, pg_stat snapshot taken
-- BEFORE the candidate queries so the audit did not contaminate its own evidence):
--
--   -- no function outside the drop set names them:
--   SELECT p.proname FROM pg_proc p JOIN pg_namespace n ON n.oid=p.pronamespace
--   WHERE n.nspname='public' AND p.prokind IN ('f','p') AND p.prolang<>12
--     AND pg_get_functiondef(p.oid) ~* 'metadata_refresh_queue|metadata_sync_log';
--   -> detect_team_change, get_metadata_queue_batch, get_metadata_queue_status,
--      mark_metadata_processed   (all four handled by this migration)
--
--   -- exactly one view, and it is dropped here:
--   SELECT c.relname FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace
--   WHERE n.nspname='public' AND c.relkind IN ('v','m')
--     AND pg_get_viewdef(c.oid) ~* 'metadata_refresh_queue|metadata_sync_log';
--   -> metadata_queue_status
--
--   -- exactly one trigger, and it is narrowed here:
--   SELECT t.tgname, t.tgrelid::regclass, t.tgenabled FROM pg_trigger t
--   JOIN pg_proc p ON p.oid=t.tgfoid WHERE NOT t.tgisinternal
--     AND pg_get_functiondef(p.oid) ~* 'metadata_refresh_queue';
--   -> trg_detect_team_change | event_box_scores | O
--
--   -- no foreign key depends on either table:
--   SELECT conrelid::regclass, conname, confrelid::regclass FROM pg_constraint
--   WHERE contype='f' AND (confrelid::regclass::text IN
--     ('metadata_refresh_queue','metadata_sync_log') OR conrelid::regclass::text IN
--     ('metadata_refresh_queue','metadata_sync_log'));
--   -> (0 rows)
--
--   -- no application code of any language names them:
--   grep -rn 'metadata_refresh_queue|metadata_sync_log|metadata_queue_status|
--             get_metadata_queue|mark_metadata_processed'
--        --include=*.rs --include=*.go --include=*.py --include=*.sh --include=*.ts .
--   -> hits ONLY in sql/metadata_system.sql (applied by nothing; sql/build.sh clones prod
--      via pg_dump and never replays it) and in sql/migrations/. No caller.
--
--   -- never drained, never written:
--   SELECT relname, n_live_tup, n_tup_ins, n_tup_upd, n_tup_del FROM pg_stat_user_tables
--   WHERE relname IN ('metadata_refresh_queue','metadata_sync_log');
--   -> metadata_refresh_queue | 42782 | 0 | 0 | 0
--      metadata_sync_log      |     0 | 0 | 0 | 0     (never once written)

BEGIN;

-- ---------------------------------------------------------------------------------
-- (A) NARROW THE PRODUCER FIRST, in-transaction, so the trigger never references a
--     dropped table. Derived from prod's current definition; the ONLY change is the
--     removal of the metadata_refresh_queue INSERT block and its now-unused `queue_id`
--     declaration. History maintenance is untouched.
-- ---------------------------------------------------------------------------------
CREATE OR REPLACE FUNCTION public.detect_team_change()
 RETURNS trigger
 LANGUAGE plpgsql
AS $function$
DECLARE
    current_team_id INTEGER;
BEGIN
    -- Skip if player_id is NULL (team stats rows)
    IF NEW.player_id IS NULL THEN
        RETURN NEW;
    END IF;

    -- Get the player's current team from history
    SELECT team_id INTO current_team_id
    FROM player_team_history
    WHERE player_id = NEW.player_id
      AND sport = NEW.sport
      AND is_current = TRUE
    ORDER BY valid_from DESC
    LIMIT 1;

    -- If no history exists (new player) OR team changed
    IF current_team_id IS NULL OR current_team_id != NEW.team_id THEN

        -- Mark old history as not current (if exists)
        UPDATE player_team_history
        SET is_current = FALSE,
            valid_until = NOW()
        WHERE player_id = NEW.player_id
          AND sport = NEW.sport
          AND is_current = TRUE;

        -- Insert new team history
        INSERT INTO player_team_history (
            player_id,
            sport,
            team_id,
            season
        ) VALUES (
            NEW.player_id,
            NEW.sport,
            NEW.team_id,
            NEW.season
        );

        -- mig 215: the metadata_refresh_queue enqueue was removed here. The queue had a
        -- producer but never had a consumer (0 of 42,782 rows processed in four months).
        -- This trigger now maintains history ONLY.

        RAISE DEBUG 'Team change detected: player_id=%, old_team=%, new_team=%',
            NEW.player_id, current_team_id, NEW.team_id;
    END IF;

    RETURN NEW;
END;
$function$;

-- ---------------------------------------------------------------------------------
-- (A) Now the queue side, consumer-first so nothing is left pointing at a missing table.
-- ---------------------------------------------------------------------------------
DROP VIEW IF EXISTS public.metadata_queue_status;

DROP FUNCTION IF EXISTS public.get_metadata_queue_batch(integer, text);
DROP FUNCTION IF EXISTS public.get_metadata_queue_status(text);
DROP FUNCTION IF EXISTS public.mark_metadata_processed(integer, boolean, text);

DROP TABLE IF EXISTS public.metadata_refresh_queue;
DROP TABLE IF EXISTS public.metadata_sync_log;

-- ---------------------------------------------------------------------------------
-- (B) NAME THE DRIVER. Each of these is invisible to a Rust/Go grep. Written as
--     COMMENT ON TABLE so `\d+` and the schema snapshot both carry it.
-- ---------------------------------------------------------------------------------

COMMENT ON TABLE public.player_team_history IS
'DRIVER: trg_detect_team_change -> detect_team_change() (trigger, AFTER INSERT on '
'event_box_scores). SQL-only: no Rust, Go, Python or shell caller. Player->team record '
'with valid_from/valid_until/is_current. Fills only while box scores arrive, so it is '
'FLAT in the off-season by design (last write 2026-06-17) — that is dormancy, not rot. '
'mig 215 kept this and dropped the metadata_refresh_queue it used to feed.';

COMMENT ON TABLE public.rating_thresholds IS
'DRIVER: _compute_rating_bundle() only. SQL-only — NO application caller in any '
'language. A grep of the Rust and Go will find nothing; the table is still live.';

COMMENT ON TABLE public.source_tiers IS
'DRIVER: backfill_narrative_episodes() only. SQL-only — NO application caller in any '
'language. A grep of the Rust and Go will find nothing; the table is still live.';

COMMENT ON TABLE public.entity_aliases IS
'DRIVERS: entity_aliases_no_update() (a trigger guard that blocks UPDATEs), the view '
'investigator_review_accepted, and rust/src/junctions/investigator/entity.rs. Partly '
'SQL-only: the write guard is invisible to a code search.';

COMMENT ON TABLE public.source_performance IS
'DRIVER (write): refresh_source_performance(), which has NO Rust or Go caller — it is '
'invoked from a psql heredoc in scripts/hosting/cron-narrative-links.sh, cron 45 */6 * * *. '
'That path is invisible to a Rust grep, a Go grep AND a repo grep for the table name. '
'DRIVER (read): source_reliability_for_pair(), called per pair by the Insider '
'(rust/src/junctions/insider/mod.rs). Refresh is DELETE-then-INSERT per sport, so high '
'n_tup_del is normal churn, not deletion of live data.';

COMMENT ON TABLE public.news_article_readings IS
'DRIVER: collapse_exact_title_duplicates(), called from rust/src/worker.rs — LIVE during '
'every ingest. Also the legacy article_read output and the rollback surface for the 30,224 '
'parked article_read rows. PHASE 9 OWNS THIS TABLE — do not drop it ahead of that demolition.';

COMMENT ON TABLE public.provider_seasons IS
'DRIVER: resolve_provider_season_id(), called from the Python seed layer (seed/shared/db.py, '
'seed/services/{roster,meta,event}/cli.py) via the cron-live-fixtures.sh jobs. NO Rust or Go '
'caller. PYTHON PRUNE SET: the seed layer is the old ingestion path and is scheduled for '
'removal now that the Investigator fetches data — retire this WITH that prune, not before.';

COMMENT ON TABLE public.provider_entity_map IS
'DRIVER: seed/shared/upsert.py:upsert_provider_entity_map(), called from '
'seed/services/{roster,meta}/cli.py via cron-live-fixtures.sh. NO Rust, Go, SQL function or '
'view names it — D-T22 pass 2 listed it as a TRUE ORPHAN and it is NOT one; Python was the '
'missing leg. 436,729 lifetime updates. Last write 2026-08-01 because the provider feeds are '
'off-season/disconnected, not because the writer died. PYTHON PRUNE SET: retire WITH the seed '
'layer, not before.';

COMMENT ON TABLE public.season_recompute_needed IS
'DRIVER: seed/shared/upsert.py — mark_season_recompute_needed() / clear_season_recompute_needed(). '
'NO Rust, Go, SQL function or view names it — D-T22 pass 2 listed it as a TRUE ORPHAN and it is '
'NOT one; Python was the missing leg. It is EMPTY and has never been written because it is a '
'durable safety queue for a failure that has not yet occurred — 0 rows is the healthy state, not '
'evidence of death. Dropping it would break upsert.py exactly when a recompute fails. '
'PYTHON PRUNE SET: retire WITH the seed layer, not before.';

INSERT INTO public.schema_migrations(version) VALUES ('215_schema_audit_metadata_and_drivers')
    ON CONFLICT DO NOTHING;

COMMIT;

-- After applying: run scripts/hosting/snapshot-schema.sh and commit sql/schema/ with this file.

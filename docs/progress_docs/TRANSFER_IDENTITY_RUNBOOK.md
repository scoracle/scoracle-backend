# Transfer Identity Operator Runbook

Applied transfer identity updates are current-identity overrides only. They must
not rewrite historical `player_stats`, `event_box_scores`, fixture, or roster
rows.

## Review Recent Applications

Use the operator CLI from `go/`:

```bash
go run ./cmd/transfer-identity list -limit 50
go run ./cmd/transfer-identity list -sport NBA -status failed_closed
go run ./cmd/transfer-identity list -sport FOOTBALL -player 37596384 -json
go run ./cmd/transfer-identity list -source-rumor-id 12345 -json
go run ./cmd/transfer-identity inspect 42
```

`inspect` includes the adjudication JSON, raw model output, threshold snapshot,
reason, evidence, and `override_id`.

Statuses:

- `applied`: an active current identity override was created.
- `reverted`: the applied override was reverted.
- `rejected`: adjudicator explicitly rejected the candidate.
- `manual_review`: adjudicator refused to auto-apply and requested review.
- `failed_closed`: deterministic threshold, JSON validity, confidence, event
  type, team consistency, or current-identity checks failed.

## Tune Thresholds

Thresholds are sport-scoped:

```sql
SELECT * FROM public.transfer_identity_thresholds ORDER BY sport;
```

Raise thresholds when too many rows reach `manual_review` or `failed_closed`
because the source is noisy. Lower them only after inspecting recent successful
applications and confirming the adjudicator evidence is consistently grounded.

Example:

```sql
UPDATE public.transfer_identity_thresholds
SET min_heat = 85,
    min_deterministic_confidence = 0.850,
    min_adjudication_confidence = 0.900,
    updated_at = NOW()
WHERE sport = 'NBA';
```

## Review Failed Or Manual Rows

```bash
go run ./cmd/transfer-identity list -status failed_closed -json
go run ./cmd/transfer-identity list -status manual_review -json
```

For each row, check:

- `reason`: the database gate that stopped the apply.
- `threshold_config`: threshold values captured at decision time.
- `adjudication` and `adjudication_raw`: strict JSON decision and source model
  output.
- `evidence`: persisted workflow evidence.
- `source_rumor_id`: link back to the transfer rumor row.

Do not manually insert `player_current_identity_overrides` for an applied
transfer. The apply path is `public.apply_transfer_identity_candidate()`.

## Revert A Bad Applied Transfer

Use the CLI so the DB function and concurrent autofill refresh both run:

```bash
go run ./cmd/transfer-identity revert 42 -by "operator@example.com" -reason "source corrected destination"
```

The command calls `public.revert_applied_transfer_identity()`, refreshes the
affected sport autofill materialized view with `CONCURRENTLY`, and prints:

- current identity before and after revert
- sport autofill version before and after
- refreshed application row

Manual verification:

```sql
SELECT *
FROM public.player_current_identity
WHERE sport = 'NBA' AND player_id = 990301;

SELECT sport, version, generated_at, total_entities, status, reason
FROM public.sport_autofill_versions
ORDER BY sport;
```

## Autofill Version And `/meta` Cache Behavior

`refresh_sport_autofill()` is now a transaction-safe invalidation shim. It marks
`sport_autofill_versions.status = 'refreshing'` inside the apply/revert DB
function. The Rust transfer stage and operator CLI then run:

```sql
REFRESH MATERIALIZED VIEW CONCURRENTLY <sport>.autofill_entities;
SELECT public.complete_sport_autofill_refresh(...);
```

Only the affected sport version should increment. Smoke check:

```bash
curl -sS https://api.scoracle.com/api/v1/nba/meta | jq '{meta_version, autofill_status, generated_at}'
curl -sS https://api.scoracle.com/api/v1/nfl/meta | jq '{meta_version, autofill_status, generated_at}'
```

Expected after a successful refresh:

- affected sport `meta_version` increments
- affected sport `autofill_status` is `ready`
- unaffected sports keep their prior `meta_version`

If a refresh fails, `sport_autofill_versions.status` becomes `failed` with the
error in `reason`. Re-run the CLI revert/list inspection or call the internal
refresh path after correcting the materialized view issue.

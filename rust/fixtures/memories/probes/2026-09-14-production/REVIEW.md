# Production Scout request: Morgan Rogers

This is the first complete production-built Scout request against the Archbox database and
configured local model. The diagnostic used the normal `build_rating_request` path and called the
provider once, while bypassing queue claims, debounce, persistence, and rewrite behavior. The
exact package, components, system/user inputs, request body, raw provider body, token counts, and
parser result are preserved in `rogers-scout-request.json`.

## Captured request

- Entity: Morgan Rogers (`FOOTBALL/player/4592198`), requested season 2026.
- Memory fingerprint: `67b36be28fad40693f7dc22a6dc344fe0d7fe43fe25559a6a586bf602d744258`.
- Generation input hash: `4741dda65b136b984672e46f393197d0`.
- Selected groups: required identity conflict and optional same-league performance comparison;
  no package omissions.
- Model: `granite4.2:3b`, temperature 0.6, 4,096-token context, 700-token output reservation.
- Exact sent body matched the body produced by the request builder.
- Actual use: 1,473 prompt tokens, 520 output tokens, `stop`, 10,131 ms.
- Parser: failed because the 2,052-character body exceeded the 1,200-character surface.

The database supplied a stale Aston Villa current-identity record beside a current Chelsea
statistical row. The comparison retained Aston Villa 2025 (37 appearances, 3,285 recorded
minutes, 10 goals, 6 assists, 32 shots on target, 48 key passes, 43 chances created) and Chelsea
2026 (one appearance, 90 recorded minutes, one assist, 0.14 xG, 1.01 xA). The Chelsea goals value
was absent and remained unknown.

## Editorial review

Rejected. The response foregrounded the stale Villa identity, converted missing prior xA to zero,
treated a one-row stored snapshot as complete playing time, claimed lower per-90 output where most
current measures were unknown, and proposed reduced role, tactical change, temporary assignment,
and limited engagement without evidence. It also exceeded the output surface.

The failure was driven by more than voice. The prompt mixed audit row references and observation
envelopes into prose, lacked current performance/roster reports, and exposed legacy current-season
percentiles whose measurement identity was empty on production schema 251.

## Source trace

Read-only repeatable-read checks found:

- Production is on migration 251; pending migrations 252 and 253 restore measurement identity and
  suppress ranks for ineligible thin samples.
- The current aggregate comes from one canonical Chelsea box-score row. Eleven other stored 2026
  Chelsea league fixtures are unverified nominations with no canonical player rows.
- `boxscore_sources` is empty and no public fixture-boxscore fetch is promoted into canonical
  `event_box_scores`/`player_stats` by the Rust investigator path.
- The FPL scheduler is running, but unresolved finished source fixtures are silently skipped before
  the reported gap count. A `gaps=0` log therefore does not establish complete fixture coverage.
- Chelsea’s retained corpus contains exact current performance evidence for Rogers from five named
  sources in the latest pair window, while the latest pair verdict is cleared at heat 43.

No source row, identity record, migration, queue item, or generated product was changed during the
capture or trace.

## Implemented response

- The complete `Package::render()` remains the audit view. `render_for_model()` now omits row keys,
  observation envelopes, ingestion timestamps, and stored schedule bookkeeping while retaining
  named teams, seasons, competition, recorded denominators, missing values, conflicts, and
  interpretation constraints. All writer junctions use this model view.
- Scout now receives attributed performance and roster claims as well as injury/suspension claims.
  The same selected claim types participate in its input fingerprint.
- Provider results expose prompt/output token counts, completion reason, and raw response body so a
  production probe can be reproduced without editing the reply.
- Migration 254 adds a `settled_sources` nomination route to the existing identity owner. It
  requires a latest-season stats row at the proposed team and exact Editor links from at least two
  named roster/performance/transfer sources. Postgres revalidates this before the existing
  fail-closed adjudicator and application/override workflow can write. Actual decayed heat remains
  in the audit record.
- Personnel context labels `applied_at` as the identity-confirmation date. It no longer presents
  that timestamp as a signing date.
- The FPL importer now adopts a matching house fixture or creates and binds the missing official
  FPL fixture, including score and gameweek provenance. An unresolved source fixture increments
  `fixtures_unmatched` and makes the pipeline partial instead of disappearing behind `gaps=0`.

The new presentation and evidence route are local and have not been deployed or applied to
Archbox.

## Qualified thin-sample follow-up

`rogers-scout-request-qualified.json` preserves the final request after the shared input repairs.
It used the same production database and configured `granite4.2:3b` route from an isolated copy of
the local Rust tree; it did not persist a product or change the database.

- Memory contract: `memories-v3`; deploying it invalidates affected writer keys once.
- Request fingerprint: `1e84d7e3d654fd66eab10ee7ad7a3a63`.
- Memory fingerprint: `fec75d6fb43936d68a59c8c39cd6b533010c4c65461696734b52a5aadb60d7d8`.
- Exact artifact SHA-256: `3079dc1a903885f40feb8b43adecd76296748ea1d0c9a01dfb06695bdc346b43`.
- Actual use: 957 prompt tokens, 192 output tokens, `stop`, 3,271 ms.
- Parser: passed; the complete body is 752 characters, within the 1,200-character surface.

The writer received the two newest exact current reports: Chelsea's account of Rogers scoring and
Yahoo Sports' account of three assists from the bench. Each is labeled with its publication date
and kept separate from the unlinked stored aggregate. Legacy schema-251 percentiles were removed
because the one-appearance sample fails the source-stat eligibility gate, and raw rating labels
with no underlying measurement identity were withheld from prose. The full values remain in the
audit package and generation fingerprint. The seasonal view named both teams and samples, then
explicitly prohibited a directional conclusion from one stored appearance.

Editorial review passed for this thin-sample case. The response identifies Chelsea as the current
playing context, attributes the goal and assist reports, treats Villa as a separate earlier sample,
and declines to infer improvement, decline, fitness, playing-time loss or ability change. It makes
no invented numeric comparison. This qualifies an insufficient-evidence reading, not a supported
role-change reading and not proof that canonical season coverage is complete.

Operational work remains: deploy migrations 252/253 before exposing ranks, deploy and exercise the
settled-source identity nomination route, run the FPL repair to rebuild canonical source coverage,
and repeat the complete check after those source facts change. Journalist and Insider qualification
still follows the reliable Scout checkpoint.

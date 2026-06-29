# Rust Cognition Harness — the Step-3+ audit

**Date:** 2026-06-29
**Scope:** the `rust/` crate (11,462 LoC, 17 src modules + 11 [[bin]] entries),
read end-to-end. Verdicts on what to **preserve**, what to **clean up**, and what to
**watch** as the system grows past the Step-3 cutover.

## Execution status (post-audit session)

- **C1 — sigil.rs private `go_json_*`/`hash_components` deduped to `util`** — done ae64efb.
- **C2 — deadcode `Stage::Momentum` dropped** — done ae64efb.
- **C3 — main.rs `[COGNITION_STAGES]` default refreshed to the production set** — done ae64efb.
- **C4 — rust/README.md rewritten for the post-Step-3 crate** — done ae64efb.
- **B1 — the four L4–L6 development-measurement bins deleted** — done e1bdcd5
  (1562 LoC out; cargo cycles ~30% faster; release.sh's cargo pass no longer re-touches them).
  `Harness::resolve_one` preserved with its HORIZON doc + the resolve.rs unit tests as the
  right amount of structural debt for the deferred transfer-subject upgrade.
- **B2 — Rust build-time commit/build-time stamp** — done via `build.rs` +
  `src/buildinfo.rs` + the boot log line in `src/main.rs`. The daemon now reports its commit
  + build-time at startup (`journalctl --user -u scoracle-cognition`), matching the Go
  `buildinfo` LDFLAGS path.
- **B3 — widen `work::Item.entity_id` to `i64`** — deferred (multi-file touch, lower-value;
  article ids fit comfortably in i32 today).
- **C5 — keep `cron-statcommentary.sh` + `go/bin/statcommentary` Step-3 rollback aid** —
  deferred until Step 3 has bedded in comfortably (user's call — say 4 weeks clean).
- **B4 — stage-port shape recipe** — landed as part of C4 (the README refresh).

The original audit findings are kept below for reference.

## Vision alignment (the brief: a foundation for something special)

The thesis the user named — **Go ingestion → Postgres data handling → Rust empowers
the local models to generate the product → Go serves the endpoints** — is **already
the architecture of this crate**, and the alignment is strong enough that I'd describe
it as a clean, simple, scalable structure on first reading:

- **Go ingestion:** `cron-pipeline.sh -mode ingest` sweeps RSS; Go enqueues `scrub`
  work + runs the daily `vibesynth` sigil-reconciliation backstop (DB-only). The Go
  derive worker is retired (`DERIVE_WORKER_ENABLED=false`).
- **Postgres data handling:** every deterministic value — `compute_transfer_heat`
  (mig 032), the `rating_breakdown` percentile/T-scores, the `debounce_unchanged` /
  `last_commentary_hash` latest-row rules — stays SQL. `rust/src/rating.rs:198-202`
  is the explicit "the deterministic stores stay in Postgres; Rust reads them,
  never recomputes" boundary, kept verbatim through the L12 port. `scrub` writes
  `vetted`, which fires the mig-103 trigger enqueuing the per-entity derive stages —
  Postgres is the choreographer, not Rust.
- **Rust empowers the models:** the crate's six primitives hang off the `Harness`
  context: `extract` (route + parser seam), `resolve_set` (asymmetric cosine + model
  gate), `embed` (CPU candle), `cluster` (deterministic union-find), `debounce_unchanged`
  + `Provenance` (persist envelope), `normalize` (HORIZON stub). The five queue stages
  (scrub/transfers/narratives/vibe/sigil) and the rating **batch** are each a thin
  composition of those primitives around their own byte-faithful port of the Go prompt.
  No stage reached up into Postgres logic; no Postgres function reached down into a
  stage. The seam is right.
- **Go serves:** the Go API reads from the precomputed `vibe_scores`, `news_summaries`,
  `stat_summaries`, `transfer_rumors`, `sigil_synthesis` tables — **one** HTTP call
  per serving request, zero model calls. The Rust daemon never answers HTTP; it only
  writes through Postgres.

The two real traits — `Inference` (the model backend; `OllamaClient` is its only impl
until vLLM lands) and `Parser<T>` (the per-stage output plug-in) — and the
`GovernedInference` decorator at the route seam (the un-bypassable GPU bound) are
exactly the swap points the Hardware Roadmap will exercise, and **every other
processor is `async fn` methods on `Harness` / on the stage** — no over-abstracted
dyn-trees, no DI container. The plan's "§5 the library was drawn right" test (resolve
dropped in BEHIND the harness signatures with no change to them) is the structural
proof, and it does hold: every method a stage calls on `Harness` was authored in L0
against the L1 vibe composition, then the L3 sigil / L10 scrub / L11 transfers / L12
rating / L13 narratives composition slotted in without touching it.

This is genuinely good work. The frictions below are cleanup + a couple of small doc
fixes, not architecture questions.

## Preserve (do not refactor)

1. **The per-stage shape** (`[stage_consts] → loaders → build_*_request (deterministic
   `X::Skipped|Ready` enum) → generate_* (extract) → persist_*`). It's copy-paste-ish
   but **the duplication is the point**: each stage's deterministic prefix is the parity
   axis with Go, and DRY-ing it would either entangle stages or make the byte-parity
   proof harder to hold. A 7th stage porting in has a recipe to follow; that's the right
   grain.
2. **`Harness.extract` sourcing `request_body` from the same backend + opts the call
   used** (`harness.rs:96-105`. Temp-0 parity leans on this — the recorded body can't
   drift from the POSTed one. Don't pull `request_body` up to the call site.
3. **Fail-closed as `Option<T>`** (transfer's `is_rumor: Option<bool>`, vibe's
   `sentiment: Option<i32>`, rating's pre-model no-stats marker). The validity IS the
   type; an uncommitted verdict is *unrepresentable* as a served row. The per-stage
   `X::Skipped | Ready` enum follows the same shape. Don't be tempted to a
   "Result<T, Marker>" — it would lose the "one type for one stage" clarity.
4. **`GovernedInference` wrapping every backend at `build_backend`** before caching /
   role-sharing. Seating the GPU bound at the seam makes it impossible to bypass — a
   future parallel drain can't sneak a call past a check in one handler. The Seahorse
   invariant "one GPU → one budget" is in the right place.
5. **The offline parity bins** (`parity`, `sigil_parity`, `rating_parity`,
   `transfer_parity`, `narratives_parity`). They're the regression suite that proved
   the cutover byte-for-byte; they live cheek-by-jowl with the port they gate and re-run
   in seconds. Keep them — they are how a future model swap (or a Rust refactor) proves
   it didn't drift.

## Cleanup (small + mechanical; do at low-risk moment)

### C1 — `sigil.rs` private `go_json_*` / `hash_components` duplicates

The L12 carry, explicitly logged. `rust/src/util.rs:50-89` already
holds `pub(crate) go_json_string / go_json_float / hash_components` — single-homed
for L12 (rating). `rust/src/sigil.rs:478-527` carries byte-identical *private* copies
(sigil authored its own first, in L3). The reason the L12 entry didn't migrate sigil:
deferred to avoid perturbing the proven L3 stage. Post Step-3 (stable, ambient
attention to the crate) is a low-risk moment to do the literal one-line edit: delete
sigil's private copies + change the call sites (sigil.rs:442, 448, 455, 458, 467,
690, 857) to `crate::util::{go_json_string, go_json_float, hash_components}`. The two
sigil.rs tests at lines 983-999 that pinned the encoder behavior become redundant
with `util.rs:96-121` — keep or delete; both fine.

Risk: zero. The encoder bytes are identical; the existing shadow-table parity gate
catches any regression.

### C2 — `work::Stage::Momentum` is deadcode

`rust/src/work.rs:31` declares `Stage::Momentum`. There is **no `MomentumHandler`,
no live enqueues of `mommentum`, and no production read of the variant** — Momentum is
served straight from a SQL view (`go/internal/db/db.go`'s `read_momentum`), and the
sigil pillar loads it from `news_summaries.vibe_score` series + composite deltas (see
`sigil.rs::load_pillars` + `linear_slope`), NOT via the queue. A search for
`Stage::Momentum` outside the enum's own `as_str` arm returns 0 hits.

The variant was set down early (Phase 1 thinking) and never used. Delete it: drop the
enum arm + the `as_str` match arm. Risk: zero — `for_role`-style totality checks don't
exist on `Stage` (the worker only registers `StageHandler`s for what `COGNITION_STAGES`
names, and Momentum was never one of them).

### C3 — `main.rs` default `COGNITION_STAGES=vibe,sigil` is stale

`rust/src/main.rs:55-60` defaults `COGNITION_STAGES` to `"vibe,sigil"` and the comment
block at lines 52-54 still says *"For the L6 scrub cutover run scrub-only
(`COGNITION_STAGES=scrub`) so the service never double-claims vibe/sigil — those stay
with the Go Drainer until their own cutover."* That history is six cuts behind (L10 →
L13 → Step 3). The post-Step-3 default *is* the production set
`scrub,transfers,narratives,vibe,sigil`, and the systemd unit
(`scoracle-cognition.service:27`) already hardcodes it — so the binary default only
fires if env is unset (a fresh-box boot without the unit, for instance).

Pick one: either (a) update the default to the production set + rephrase the comment to
"Step-3 done — the daemon owns all five stages; Go Drainer is retired," or (b) drop the
default entirely so a misconfigured boot fails loud instead of running two stages and
silently under-deriving. (a) is gentler; (b) is safer. Recommend (a) now, (b) in a
few weeks once Step 3 is fully bedded in.

### C4 — `rust/README.md` is severely stale

The README (last meaningful update Phase 1) describes a Phase-0 host with no handlers,
claims `cargo build` is "not yet verified on this machine", and pre-tells a "library-
first" plan whose execution (L0-L13 + Step 3) is now in `progress_docs/`. A new reader
landing here today gets a worse mental model than opening `lib.rs`. Worth a single
refresh: aim it at "what this crate is post-Step-3" (the 5 stages + the rating batch,
the Harness primitives, the parity gates as the regression suite, the live ops path via
`scoracle-cognition.service` + `release.sh`), and link out to `progress_docs/`
for the build ledger. This is a ~30-minute doc PR; not blocking anything.

### C5 — `scripts/hosting/cron-statcommentary.sh` + `go/bin/statcommentary` Step-3 rollback aid

`RUNBOOK.md` (today's update) names these as the **deliberately-preserved rollback
aid** for the Step-3 cognition cutover: re-enable Go derive + restore `crontab` and Go's
batch rating binary picks back up. Once Step 3 has been live comfortably for a
defined window (user's call — say 4 weeks clean), the cleanup is to delete both files +
remove the `cron-statcommentary.sh` row from `README.md` and `crontab.example` if it's
listed. **Not now** — Step 3 is fresh (2026-06-28).

## Bigger observations (questions for the next session, not actionable today)

### B1 — the L4-L6 development-measurement bins are throwaway scaffolding

`Cargo.toml` builds **11** `[[bin]]`s. Five are the parity gates (C5 — keep). One is
the live daemon `scoracle-cognition` + one is `statcommentary` (live batch). One is
`eval.rs` (the A/B role-eval harness — live and useful, the way you'd pick a candidate
model). The other **four** are the L4-L6 development-measurement harnesses for the
embedding Resolve gate:

- `resolve_experiment.rs` (L4 — de-risk: do cosine CSS separate genuine labels?)
- `resolve_eval.rs` (L4 — the real gate's agreement vs `vetted`)
- `resolve_shadow.rs` (L5 — at-scale auto-keep precision + auto-drop FN rate)
- `redundancy_check.rs` (L6 — is each dropped genuine a near-dup of a kept article?)

These answered their questions: the keep/drop bands got settled (`ResolveConfig`
defaults 0.75 / 0.60, AUC 0.88), the asymmetric gate went live, and the redundancy
finding shaped the "proxy never excludes" policy. **They are not regression suites** —
they're measurement scripts that pointed at a specific moment, and they're built on
every cargo run (every developer/cargo cycle, plus integrated into release.sh today).
Two options:

- **Delete them**, keep their source pinned in git history for archaeology. The
  `Harness::resolve_set` they exercised is covered by `resolve.rs`'s own
  `relevance_parser_fail_closed` + `classify_is_asymmetric` unit tests; the bands
  themselves are config, not code. Clean, simple, scalable — what the audit asks for.
- **Move them under `examples/`** so cargo skips them by default but they stay in-tree.
  Lower-effort than deleting + reverts easily.

Either way: gain is `cargo build` skips ~1500 lines of throwaway dev code on every
cycle, and `release.sh`'s cargo pass stops re-touching them.

The **open question** for `Harness::resolve_one` (the L4-transfer-subject-test HORIZON
primitive in `resolve.rs:213-264`): the only production user of `resolve_one` was
`resolve_eval.rs`. If the four dev bins go, `resolve_one` stops being exercised by any
bin — but it stays documented as the HORIZON transfer-subject upgrade (transfers'
`transfer.rs:16` docstring explicitly explains it's deferred to "avoid splitting the
one fused call into two and breaking Go-machinery parity"). Recommendation: keep
`resolve_one` with its existing HORIZON doc comment + the two resolve.rs unit tests
(classify + parser). Moving the bins away does not orphan it; the doc says what would
use it and why it isn't, which is the right amount of structural debt.

### B2 — Rust crate doesn't stamp commit/build-time; Go does

The Go binaries carry a `commit` + `build_time` via LDFLAGS, queryable at `GET /` and
logged at boot — the authoritative "what's deployed" check for the Go side. The Rust
binaries don't. Today's `release.sh` ships them from the same commit, so the commit IS
the API's served commit; but if the API is healthy and the daemon is wedged, there's
no `scoracle-cognition --version` to confirm which build is running. An `env!("CARGO_PKG_VERSION")` + a `build.rs` reading `git rev-parse --short=12 HEAD` into a
`const` would close this. Not urgent; flag for a future tidy.

### B3 — `work::Item.entity_id` is `i32`; scrub casts to `i64`

`work.rs:58` (`entity_id: i32`) serves every stage except scrub — which is
article-keyed, and `scrub.rs:58` does `i64::from(item.entity_id).` Article IDs fit
in i32 (< 2bn), but if a fresh ingestion grows past that the cast silently wraps.
Two paths: (a) widen `Item.entity_id` to `i64` (touches every stage's `entity_id`
binds; harmless but it's a multi-file diff), or (b) just leave it + add a one-line
assertion to `scrub.rs`. (a) is the cleaner long-term answer; (b) is the cheaper one.

### B4 — The stage-port shape is undocumented except by reading

A 7th-stage port (e.g. the future notifications-NLP stage, or a sci/ML-derived "trade
market heat" stage) gets the shape only by reading any of `vibe.rs` / `rating.rs` etc.
A ~80-line `rust/src/stage.rs` expansion (or a `STAGE-RECIPE.md` next to it) laying out:
*the deterministic `Build::Skipped|Ready` split, the `extract` composition, the
`Provenance` envelope with `input_hash` for debounce, the `*Parser` plug-in, and the
"declare the role your prompt speaks to" rule* would make the next stage port a copy-
paste-with-confident-edges, not a forensics exercise. Same observation as the README
refresh (C4) — it's a doc problem, not a code problem.

## Other findings worth logging only

- **`Harness.normalize` `unimplemented!()` stub (harness.rs:368-370)** — the multilang
  HORIZON primitive. Six Ls old and still a stub. Cheap to keep; it documents the
  boundary. Leave.
- **`pub mod` for every module + no `prelude`** — fine for a crate this size. Don't
  over-engineer.
- **`#![allow(dead_code)]` Phase-0 flags in the README** — `grep` shows the crate does not
  actually carry that allow any more; the README is the stale part (C4 already covers it).

## Recommended session (the audit, sequenced)

1. **C1** (sigil go_json_* dedup) — literal one-liner.
2. **C2** (drop `Stage::Momentum`) — one enum arm + one match arm.
3. **C3** (main.rs default — pick (a), the gentle update) — three lines + a comment.
4. **B1** (the four L4-L6 bins) — delete (preferred) and gain faster cargo cycles; or
   move to `examples/`. Either way, also prune `lib.rs`/Cargo.toml `[[bin]]` entries.
5. **C4 + B4** (rust/README.md refresh + stage-port shape recipe) — one doc pass tying
   the C1-C3 + B1 cleanup together into "here is the post-Step-3 crate."
6. **B3** (entity_id widening) — optional, do it the next time you're in work.rs.

C5 (the legacy Go statcommentary backup aid) waits on Step-3 bed-in time. B2 (Rust
build-stamp) is a separate future tidy.

The audit session would not change architecture — the architecture is right. The work
is **delete + dedupe + doc**, executed in the listed order, with the cliff test at the
end (`cargo build && cargo clippy --all-targets -- -D warnings && cargo test --lib`,
all 80 tests still pass).

## File-layout delta (if everything here is applied)

```
rust/src/sigil.rs                − ~50 lines (drop the dup go_json_*/hash_components; tag the existing tests obsolete or fold into util's)
rust/src/work.rs                 − 2 lines (Stage::Momentum arm + as_str arm)
rust/src/main.rs                 ~3 lines (default + comment refresh)
rust/Cargo.toml                   − ≤4 [[bin]] entries (B1 delete path)
rust/src/bin/resolve_experiment.rs   DELETED (B1)
rust/src/bin/resolve_eval.rs        DELETED (B1)
rust/src/bin/resolve_shadow.rs      DELETED (B1)
rust/src/bin/redundancy_check.rs    DELETED (B1)
rust/README.md                      rewrite (C4)
rust/src/stage.rs (or new STAGE-RECIPE.md)   +~80 lines (B4)
```

Net: ~1700 lines out (mostly throwaway dev bins), ~80 lines in (doc). Crate lands at
~9.7k LoC and reads as a clean, deliberate cognition layer.
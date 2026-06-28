# Rust Cognition Harness — L12: rating port (Cutover Step 2, part 2)

**Date:** 2026-06-28
**Plan:** vault `Plan - Rust Cognition Harness build.md` → "The Cutover Plan" (Step 2) + §7 ledger (L12)
**Status:** DONE — the **rating** stage is ported OFFLINE into Rust, parity-gated **6/6** on the
deterministic axes (full byte/jsonb parity, `system` included — a faithful port, no t4-style
divergence). No live impact: the offline parity bin writes only the shadow table; nothing in Go is
retired. Rating has **no `pipeline_work` queue stage** — it is the `cmd/statcommentary` BATCH — so
there is no handler to register; the cutover artifact is a Rust batch bin (Step 3).

## Goal

Execute Cutover Step 2 (continued) for **rating** — the stats-rail on-field IDENTITY commentary.
Port `go/internal/ml/rating.go` + the per-entity core of `cmd/statcommentary` into Rust, composed as
`route(StatsLogic) + extract + persist` (rating is the FIRST `Role::StatsLogic` consumer), and prove
machinery parity with Go offline before any cutover. Hold the L8 breakthrough: the model VERBALIZES
the deterministic tier label, it never maps percentile→quality itself.

## Accomplishments

### 1. The rating port — `rating.rs`

Faithful port of `rating.go` at the per-entity grain. The deterministic STORED stats stay in Postgres
and are READ (`load_rating_profile` SQL is **verbatim** Go, only `::float8` casts added on the
`numeric` score columns + `::text` on the JSONB): `rating_composite_score`, `rating_specialist_score`,
the `rating_breakdown` percentiles (`pct`/`z`) — all Postgres-computed. The transient **prompt-shaping**
(notability, `pctBand`, `trimFloat`, ordered facts, rate standouts) is mirrored in Rust **byte-for-byte**.

- **`build_stat_prompt`** is byte-identical to Go's `buildStatPrompt` (the `·` U+00B7 / `—` U+2014 are
  significant bytes; `%.0f`/`%+.1f`/`trimFloat` map to Rust `{:.0}`/`{:+.1}`/the same casing).
- **`RATING_SYSTEM_PROMPT`** (s6) is carried **verbatim** from Go (no single-home bump — unlike L11
  transfers' t4), so the WHOLE `ollama_request` including `system` is a parity axis.
- **`input_components` + `hash_components`** reproduce Go's `(*ratingProfile).inputComponents` +
  `hashComponents` byte-for-byte (sorted keys, HTML-escaped strings, Go's shortest float form) — the
  **5th parity axis transfers lacked** (rating debounces on `input_hash`; a drift would spuriously
  regen the corpus at cutover). Built with a tiny `GoJson` value emitter over the shared `util::go_json_*`.
- **The L8 breakthrough preserved.** `pct_band` (the percentile→tier mapping) is **deterministic
  prompt-shaping** that lives in the STAGE — the rating.go comment is explicit ("like sigil's
  `trendDir`, NOT a stored derived stat"), so by the transfers precedent (mirror the prompt assembly
  in Rust; keep stored stats in Postgres) it is mirrored in Rust, exactly as Go does it. *This is the
  faithful reading of the handoff's "keep pctBand in Go/Postgres" — the constraint is "deterministic,
  NOT the model's job", which the Rust mirror honors (the model verbalizes the labeled tier; the gate
  proves the tier labels are byte-identical).* Moving `pctBand` into Postgres was rejected: it would
  edit the frozen Go side + need a data-gated rating-engine migration + break the faithful-port gate.
- **Composition:** `build_rating_request` (deterministic prefix — the parity axes, no model) →
  `generate_rating` (`extract(StatsLogic)` + the `RatingParser` peak/body split + clean + the
  skip-unchanged debounce) → `persist_stat_summary` (→ stat_summaries; written-not-run this session).
- **Fail-closed:** rating's ONLY marker is the PRE-model no-stats path (no usable rating row → a
  NULL-body marker, like vibe's no-corpus). No post-model marker — an empty body is a hard error
  (`RatingParser` never returns `Ok(None)`, like `VibeParser`).
- **No `RatingHandler`** — rating is not a `pipeline_work` stage (no `Stage::Rating`); the cutover is a
  batch bin (Step 3), so registering a handler would be wrong-shaped. The per-entity core
  (`generate_rating` + `persist_stat_summary` + `last_commentary_hash`) is production-complete; only the
  CLI enumeration wrapper (backfill/nightly) is deferred.

15 unit tests: 2 byte-fixture `build_stat_prompt` tests (player w/ composite + scoped position; team
w/o composite), the `input_components` canonical-JSON fixture (the `input_hash` pre-image, byte-exact
vs Go's marshal), the **null-tolerance** regression (below), `pct_band`/`trim_float`/`compute_notability`/
`ordered_facts`, and the parser (peak/body split, legacy SIGIL: prefix, clean).

### 2. Shared Go-JSON helpers single-homed in `util.rs`

`go_json_string` (HTML-escaped like Go's default), `go_json_float` (shortest 'f' form, no ".0"), and
`hash_components` (SHA-256 → 128-bit hex) added to `util.rs` as `pub(crate)` (with tests) — the leaf
encoders an `input_hash` matching Go's `hashComponents` needs. sigil.rs still carries byte-identical
private copies (a **noted one-line cleanup** — migrate sigil to these — deferred to avoid perturbing
the proven L3 stage; matches the L11 precedent of adding new shared helpers to util without refactoring
existing stages).

### 3. The parity gate — 6/6 players on all 5 deterministic axes

- **mig 111 `stat_summaries_shadow`** — the throwaway diagnostic (no FK/trigger), applied **surgically**
  (`psql --single-transaction` + ledger INSERT for ONLY 111, never `migrate.sh` while 099 is untracked).
- **`bin/rating_parity`** (source='rust') + **`go/internal/ml/rating_parity_test.go`**
  (`TestRatingParityDump`, source='go') — both dump the deterministic axes for the SAME entities.
- **NO model call for the gate** (the L2 finding): the axes are all deterministic; the prose is not a
  temp-0 parity axis.
- **Result (6 players across NBA / NFL / FOOTBALL, composites 69–98, rich profiles w/ rate modes +
  scoped positions + a null-bearing sparse datapoint): 6/6** on `built_prompt` byte-equal, the WHOLE
  `ollama_request` jsonb-equal (`system` INCLUDED), `model_version`, `prompt_version` (s6), AND
  `input_hash`. The cleaner gate vs transfers (no intended divergence to exclude).
- **Vet run** (`RATING_PARITY_VET=1`, live mistral via the governor, 2 players) validated the FULL path
  end-to-end + the L8 win: Wembanyama's 62nd-pct three-point shooting → "merely above average"
  (`pct_band(62)`), his 100th rim protection → "elite", 15th ball security → "significant weakness";
  Garrett's 81st tackling → "strong", 54th pass defense not over-praised. `divined_peak` extracted
  ("Elite rim protection…", "Elite pass rush"), single-flowing-paragraph analyst voice. **The model
  verbalizes the labeled tier — it does NOT re-derive percentile→quality.**

**Gate — PASSED.** `cargo build` 0 warnings · `cargo clippy --all-targets -- -D warnings` clean ·
`cargo test --lib` all pass (15 new rating + 3 new util) · `gofmt`/`go vet` clean on the new Go test.

## Findings the gate surfaced

- **Teams are DORMANT in this stage (a latent Go bug, faithfully reproduced).** `team_stats` lacks the
  `rating_modes` column (only `player_stats` has it), so Go's verbatim `loadRatingProfile` `SELECT`s a
  nonexistent column and **errors on every team** — confirmed: **0 team rows** in 4950 `stat_summaries`
  (all players). Go's `cmd/statcommentary` enumerates teams (team_stats has `rating_composite_score`
  rows) and silently fails each one every run. The Rust port reproduces the error identically (same SQL),
  so PARITY HOLDS in the degenerate sense; the gate is run on PLAYERS (the live corpus). **The cutover
  is the natural place to FIX team commentary** (select `rating_modes` only for `player_stats`) — a
  post-parity IMPROVEMENT, deliberately NOT smuggled into this faithful port (it would diverge from Go
  and break the "identical machinery" gate). Flagged for the user.
- **`null` in `rating_breakdown` — a real port bug, found + fixed.** A sparse datapoint can carry an
  explicit `"value": null` (e.g. Luka Modrić's "Penalties Won"). Go's `encoding/json` tolerates null
  (keeps the zero value); serde's `#[serde(default)]` covers only a MISSING field, so serde **errored**
  where Go succeeds. Fixed with `null_to_default` (every scalar) + `null_tolerant_map` (`scoped_pct`:
  null map → empty, null value → 0.0) — reproducing Go's null semantics exactly. After the fix, the
  null-bearing player:268 matches Go (hash `a2504c…`). Regression-tested. This likely affected many
  sparse-stat entities, so it was essential for broad parity.

## Landmines (carry)

- **`numeric` → f64 for composite/specialist** — Go's pgx decodes `numeric→float64`; Rust casts
  `::float8`. A sub-ULP difference is theoretically possible, but every consumed value is rendered/
  rounded (`%.0f` in the prompt, `round1` in the hash, the integer notability), so a boundary flip is
  astronomically rare on real T-scores — and **empirically the gate's 6/6 `input_hash` matches prove
  the round1(composite) bytes agree** for this set (incl. composites 69.4 / 95.6 / 98.0). A future
  boundary-case diff on one entity would be representation variance at a tie, not a code bug.
- **rating is a BATCH, not a queue stage** — no `Stage::Rating`, no `RatingHandler`. Its cutover (Step 3)
  is a Rust batch bin enumerating `player_stats`/`team_stats` + looping `generate_rating` (the per-entity
  core, production-complete here). `persist_stat_summary` is written + compiles but does NOT run this
  session (offline); first live run is the cutover.
- **sigil.rs still has private `go_json_string`/`go_json_float`/`hash_components` copies** — byte-identical
  to util's; a one-line future cleanup (migrate sigil to `util::`), deferred to not perturb L3.
- `099_team_rosters.sql` still untracked (not ours; already applied+recorded in `schema_migrations` by
  its owner). **F-046 still OPEN** (DB password in git history; a purge rewrites the cognition commits).
- **archx220 `ollama pull mistral:7b` — DROPPED** (user-directed 2026-06-28): archx220 is the laptop;
  Ollama runs on **archbox** (the serving box, where cognition runs). No cross-machine pull needed.

## Quick reference

```bash
# Build + the gate:
cargo build --manifest-path rust/Cargo.toml
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path rust/Cargo.toml --lib

# Rating parity (env from .env.local: DATABASE_PRIVATE_URL + OLLAMA_MODEL); run TIGHT back-to-back:
SPECS="player:56677822:NBA player:246:NBA player:63:NFL player:1298:NFL player:997:FOOTBALL player:268:FOOTBALL"
./rust/target/debug/rating_parity $SPECS                                    # source='rust' (s6), deterministic
( cd go && RATING_PARITY_DB=1 RATING_PARITY_ENTITIES="$SPECS" \
    go test ./internal/ml/ -run TestRatingParityDump -count=1 )             # source='go' (s6)
# Diff: DISTINCT ON (source,entity) latest row → built_prompt + ollama_request (whole jsonb) +
# model_version + prompt_version + input_hash all equal. (Teams error on both sides — dormant stage.)

# Full-path vet (live mistral): RATING_PARITY_VET=1 ./rust/target/debug/rating_parity player:56677822:NBA
```

## File layout delta

```
rust/src/util.rs                      + go_json_string / go_json_float / hash_components (pub(crate)) + 3 tests
rust/src/rating.rs                    NEW — the rating port (loader + deterministic + s6 prompt + input_hash + parser + compose + persist)
rust/src/lib.rs                       + pub mod rating
rust/src/bin/rating_parity.rs         NEW — the source='rust' parity dump (deterministic; --vet optional)
sql/migrations/111_stat_summaries_shadow.sql   NEW — the shadow table (applied surgically)
go/internal/ml/rating_parity_test.go           NEW — the source='go' parity dump
```

## Next — Cutover Step 2 (continued): L13 narratives, then Step 3

**L13 — narratives.** The largest + heaviest GPU stage: compose `embed+cluster` (the candle dedup —
group storylines + drop near-dups ≥~0.85 cosine, the genuine Rust value-add) + route + extract +
persist. Port `go/internal/ml/news_narratives.go`; parity-gate like rating/transfers (deterministic
axes; a `*_shadow` table = mig 112).

**Step 3 — full cutover** (after narratives lands + is parity-proven): `COGNITION_STAGES=scrub,
transfers,narratives,vibe,sigil` (+ rating via its batch bin), `DERIVE_WORKER_ENABLED=false`, retire
the Go cron drainer + inline scrub + statcommentary batch. Rust = sole GPU user. **Consider fixing team
rating commentary here** (the dormant-stage finding above). vibe/sigil fold in.

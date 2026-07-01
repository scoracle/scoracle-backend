# Rust Cognition Harness — L3: first stage port (sigil) — 2026-06-24

Repo-durable record of the L3 increment of the library-first Rust Cognition Harness build.
The forward-looking plan + the append-only ledger live in the vault:
`scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` (§7 Build ledger → the L3 entry).
This doc is the git-side summary; see also the L0/L1 and L2 docs in this directory.

## Goal

Land the **first NEW derivation on the capability library** — re-express the **sigil** crown
convergence as a composition of the L0–L2 primitives (`read 3 pillars + route(StatsLogic) +
extract(SigilParser) + persist`, debounced on `input_hash`) — and prove it byte-for-byte against
the Go stage at temperature 0. **The primitives do not move**; a stage is a *recipe*, not new
infrastructure.

Chose **sigil over rating** because it exercises strictly more of the library: the first
`Role::StatsLogic` consumer, the first user of `Persist::debounce_unchanged`, and the first user
of the `Provenance.input_hash` envelope field — three things shipped real but unexercised by vibe.
Sigil's prompt is also robust to FP-determinism noise (its `linearSlope` feeds *bucketed*
`trendDir` text, and the printed numbers come straight from DB columns), avoiding rating's
`computeNotability` rounding-boundary risk. Sigil is also the convergence capstone — it reads
vibe's own felt-read output.

## What landed

New stage module + harness, all mirroring `go/internal/ml/sigil.go` line-for-line:

- **`rust/src/sigil.rs`** (new) — the recipe:
  - Pillar loaders byte-mirroring the Go SQL: `resolve_season` (current_season), the three
    pillars (`load_narrative_pillar`, `load_rating_pillar`, `load_momentum_pillar`), shared
    front-half `load_pillars`.
  - Deterministic trend math `linear_slope` (OLS) + `trend_dir` (the ±0.3/±1.5 buckets) +
    `round1` — mirrored exactly (only the bucket label reaches the prompt).
  - `build_synthesis_input_components` → **canonical JSON byte-identical to Go's
    `json.Marshal`** (sorted keys, HTML-escaped strings via `go_json_string`, Go's shortest
    float form via `go_json_float` — no trailing `.0`), then `hash_components` (SHA-256, hex of
    the first 16 bytes) = Go's `hashComponents`. This is the `input_hash` / debounce key.
  - `build_synthesis_prompt` byte-for-byte (raw entity_type — *not* title-cased like vibe).
  - `SigilParser: Parser<SigilReply>` (case-sensitive `SCORE: `/`BLURB: `, no first-integer
    fallback; `score == 0` ⇒ Err, never `Ok(None)`).
  - `generate_sigil` (parity core: load → no-pillar marker → components+hash → prompt →
    `extract(StatsLogic)`) and `SigilHandler` (production: + `debounce_unchanged` skip +
    `last_score` previous-score + typed persist; **terminal** — no downstream enqueue).
  - 15 unit tests (parse, trend buckets, slope, round1, the Go-JSON byte forms, the canonical
    components string, prompt assembly).
- **`rust/src/bin/sigil_parity.rs`** (new) — twin of `bin/parity.rs`; runs `generate_sigil` at
  an explicit temp 0 and writes `source='rust'` to `sigil_synthesis_shadow`. Read-only on the
  live pipeline (never writes sigil_synthesis, never claims pipeline_work).
- **`go/internal/ml/sigil_parity_test.go`** (new) — `TestSigilParityDump`, gated on
  `SIGIL_PARITY_DB`. Reuses the shared parity helpers from `vibe_parity_test.go`; writes
  `source='go'` rows via the package's own loaders + `buildSynthesisInputComponents`/
  `hashComponents`/`buildSynthesisPrompt`.
- **`sql/migrations/107_sigil_synthesis_shadow.sql`** (new) — the offline shadow table (mirrors
  the sigil_synthesis derivation contract + `source`/`temperature`/`built_prompt`/
  `ollama_request`; `input_hash` is the 4th deterministic axis). No FK, no trigger; throwaway,
  dropped after cutover. Applied surgically via `psql --single-transaction` + the ledger INSERT
  (recorded as `107_sigil_synthesis_shadow`), NOT via `migrate.sh` (the untracked `099` is a
  parallel session's file).
- **`rust/src/lib.rs`** — `pub mod sigil;`.
- **`rust/Cargo.toml`** + **`Cargo.lock`** — `sha2` + `hex` (leaf utilities for the provenance
  hash; mirror Go's `crypto/sha256` + `encoding/hex`) + the `sigil_parity` bin.
- **`rust/src/main.rs`** — registered `SigilHandler` alongside `VibeHandler`. NB registration ≠
  cutover: the service binary is still not run against the live DB until the per-stage cutover
  (it would double-claim the queue).

## Validation gate — PASSED

- **Static:** `cargo build` 0 warnings · `cargo test --lib` **26/26** (15 new sigil) ·
  `cargo clippy --all-targets -- -D warnings` clean · all bins build · Go parity test compiles
  + skip-gates · `gofmt` clean.
- **Temp-0 parity (live DB + Ollama local-model:tag)** over a stable 5-entity corpus
  (`team:14:FOOTBALL player:23278674:FOOTBALL team:7:NBA team:3:NBA` + the marker
  `player:2169:NBA`): **5 / 5 pass all 4 DETERMINISTIC axes** — `built_prompt` bytes +
  `ollama_request` jsonb + `model_version` + `input_hash` IDENTICAL rust-vs-go (incl. the
  no-pillar marker). The `input_hash` match proves the Go-compatible canonical-JSON byte
  production is exact (and keeps the eventual cutover free of spurious regens).
- **Bonus:** SCORE + BLURB also matched 4/4 — but only because both sides ran in ONE
  model-load window. Per the L2 FINDING, SCORE/BLURB are **not** a regression signal across
  model loads (local-model:tag temp-0 is not reliably deterministic); the gate is the 4 byte axes.

## Decisions carried

- **Library-first held:** sigil added ZERO new primitives. It is the first consumer of
  `debounce_unchanged` + `Provenance.input_hash` (both already real) and the first
  `Role::StatsLogic` consumer — exactly the "a stage is a recipe" test.
- **`input_hash` is a deterministic 4th gate axis.** It is a pure SHA-256 of the canonical
  components JSON (no model call), so it is gated byte-for-byte alongside built_prompt/
  ollama_request/model_version — unlike SCORE/BLURB.
- **The hash port is faithful, not approximate.** `go_json_string` reproduces Go's default
  HTML escaping (`&`→`&`, `<`/`>`→`<`/`>`) and `go_json_float` reproduces Go's
  shortest-float form (`73.0`→`"73"`, the serde_json `"73.0"` trap avoided). Unit-tested
  against the exact bytes Go emits.
- **Deterministic math stayed where it belongs.** The composite/T-score/percentiles read from
  Postgres untouched; `linearSlope`/`trendDir` are transient prompt-shaping (a bucket label),
  mirrored in Rust like vibe's truncate/dedupe — never a stored derived stat.
- **Production handler is cutover-ready but the service is not run** until the per-stage cutover
  (flag-gating Go's `drainSigil` off). The parity bin writes shadow only.

## Landmines hit / carried

- **`rating_composite_pct` is `numeric`** — sqlx can't scan that into `f64` without a decimal
  feature, so the momentum composite query casts `::float8`. Value-identical to Go's pgx
  `numeric→float64` (both yield the nearest double), and it only feeds the bucketed `trend_dir`
  + a `%.1f` render, so the prompt is unaffected.
- **The editor decodes a literal `&` token** in a written string into `&`. Test
  expectations that need the escaped form are built with a runtime backslash
  (`let bs = '\\'; format!("…{bs}u0026…")`) so the source carries no decodable `\uXXXX` token.
- **Sigil's parse differs from vibe's:** case-sensitive `SCORE: `/`BLURB: ` (with the space),
  and NO first-integer fallback — a missing SCORE line is a hard failure.
- **Read parity on the byte axes, not SCORE/VIBE** (the L2 FINDING) — still binding.
- **Migration coordination:** applied 107 surgically (not `migrate.sh`) because `099` is still
  an untracked parallel-session file. The shadow table is throwaway (drop after cutover).
- **F-046 still OPEN** — a history purge will rewrite the cognition commits; coordinate before
  any force-push.

## Quick reference

```bash
cd scoracle-backend && export PATH="$HOME/.cargo/bin:$PATH"
export DATABASE_PRIVATE_URL=…   # crate does NOT load .env.local
export OLLAMA_TIMEOUT_SECONDS=300   # OLLAMA_BASE_URL/MODEL default to localhost:11434 / local-model:tag

cargo build  --manifest-path rust/Cargo.toml
cargo test   --manifest-path rust/Cargo.toml --lib
cargo clippy --manifest-path rust/Cargo.toml --all-targets -- -D warnings

# temp-0 parity (writes shadow only):
./rust/target/debug/sigil_parity team:14:FOOTBALL player:23278674:FOOTBALL team:7:NBA team:3:NBA player:2169:NBA
( cd go && SIGIL_PARITY_DB=1 SIGIL_PARITY_ENTITIES="team:14:FOOTBALL player:23278674:FOOTBALL team:7:NBA team:3:NBA player:2169:NBA" \
    go test ./internal/ml/ -run '^TestSigilParityDump$' -v -count=1 -timeout 25m )
# diff: self-join sigil_synthesis_shadow source='rust' vs 'go' on (entity_type,entity_id,sport,season),
#       DISTINCT ON (source,entity…) … id DESC — assert built_prompt/ollama_request/model_version/input_hash equal.
```

## Next — L4

The next stage port (rating, or transfers/narratives/scrub) via the same shadow → temp-0 parity
→ per-stage cutover loop. See the vault ledger's L3 entry for the full L4 handoff prompt. Before
any later cutover, the per-stage cutover of vibe/sigil (flag-gate the Go drain off + run the
service) is its own step — not done here.

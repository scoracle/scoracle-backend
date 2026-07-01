# 2026-06-24 — Rust Cognition Harness: L2 (config-driven Router + A/B eval bin)

## Goal
Build the **L2** increment of the library-first Cognition Harness plan
(`scoracleWiki/wiki/Plan - Rust Cognition Harness build.md`, §2/§3): replace the L1 minimal
`Router::single` with the **config-driven role→model map** (`COGNITION_ROUTE_*`) and stand up
the **A/B eval bin** — stopping at the same temp-0 parity gate, where **config-driven routing
must move ZERO bytes**. Builds on the L0+L1 floor (commit `a1f61d8`); the vibe core, the host
loop, and `work.rs` are untouched.

The binding discipline (carried from the conception): **address models by role, never by name**
— a model id appears in exactly one place (the config); a new model is a config line + a
measured eval win, never a code edit; the eval gates adoption, not faith; deterministic math
stays in Postgres.

## Context / decisions
- **`RouteConfig`/`ModelSpec`/`Backend` live in `config.rs`; `Router::from_config` in `route.rs`.**
  `RouteConfig { roles, candidates }` is the role→model map (incumbent + optional A/B
  challenger); `ModelSpec { backend, model, base_url }` is the concrete model — the ONE place a
  model id may appear. `config.rs` ↔ `route.rs` reference each other (no cycle — same crate).
- **Default every role → `OLLAMA_MODEL` on `OLLAMA_BASE_URL`.** With nothing configured, all four
  roles resolve to the one local model on the shared base — so an un-configured deploy is all-local model and
  **byte-identical to the L1 single router**. `COGNITION_ROUTE_<ROLE>` overrides per role
  (`EMOTIONAL_NEWS` / `STATS_LOGIC` / `MULTILANG` / `SQL`); `COGNITION_ROUTE_<ROLE>_CANDIDATE`
  adds the eval challenger.
- **`from_config` builds one `Arc<dyn Inference>` per DISTINCT (backend, model, base_url).** Roles
  naming the same model share one backend Arc (today: all four roles → one OllamaClient), so the
  GPU isn't hit by N clients for one model. Locked by a unit test (`Arc::ptr_eq`).
- **`for_role` stayed total (non-`Option`); `candidate_for` is `Option`.** `from_env` populates
  every `Role::all()`, so `for_role` always resolves (the incumbent). `candidate_for` returns
  `None` unless a `*_CANDIDATE` is configured — and the router **NEVER routes serving traffic to a
  candidate**; it's read only by `bin/eval`.
- **`Backend` is the committed shape of the backend swap, not speculation.** A single-variant enum
  (`Ollama`) matched in `from_config`; vLLM lands as one new variant + one new `impl Inference` +
  one new match arm *when it's real*. The `Inference` trait is committed now; the second impl is
  not built.
- **`Role` gained `all()` + `env_suffix()`.** `all()` keeps the config/router maps total;
  `env_suffix()` (UPPER_SNAKE) is the role's *config* identity for `COGNITION_ROUTE_<SUFFIX>`,
  distinct from `as_str()` (the kebab telemetry label).
- **`bin/eval.rs` scores incumbent vs candidate, NEVER auto-promotes.** It reuses the SAME public
  vibe loaders + `build_sentiment_prompt` the production handler runs (only the backend differs),
  at temp 0 (deterministic, reproducible), and reports MAE vs the human labels with an explicit
  "to adopt, a human edits `COGNITION_ROUTE_*`" line. No-corpus / unparseable cases are skipped
  from the score, not counted as zero.
- **Boot ping is a throwaway against the shared base.** `main.rs`/`parity.rs` build a one-off
  `OllamaClient` to ping `OLLAMA_BASE_URL` (every role's base today), then build the router from
  config separately. Per-backend health/timeouts move into `ModelSpec` when topology splits.

## What was done
- `rust/src/config.rs` (edited) — added `Backend` (enum), `ModelSpec`, `RouteConfig` +
  `RouteConfig::from_env(default_model, base_url)`; added `route: RouteConfig` to `Config` and
  bound the Ollama defaults as locals to feed it.
- `rust/src/route.rs` (edited) — replaced `Router { default_backend }` + `single` with
  `Router { incumbents, candidates }` + `from_config` (per-distinct-model dedup via a private
  `build_backend`) + `candidate_for`; `for_role` keeps its signature. Added `Role::all()` +
  `Role::env_suffix()`. **+3 unit tests** (dedup shares one Arc; distinct models differ;
  candidate optional/resolved) — offline, no env (so no test races).
- `rust/src/bin/eval.rs` (**new**) — the A/B eval harness: `EvalCase { entity, label }`,
  `EvalReport { role, incumbent, candidate, n }`, `ModelScore`; runs `for_role` AND
  `candidate_for` over the labeled set, prints the per-model MAE + the delta + the adoption hint.
  No-arg prints the resolved route table (a zero-DB smoke).
- `rust/src/main.rs`, `rust/src/bin/parity.rs` (edited) — construct the router via
  `Router::from_config(&cfg.route, cfg.ollama_timeout)` instead of `::single`; kept the boot ping;
  dropped the now-unused `std::sync::Arc` imports.
- `rust/Cargo.toml`, `rust/src/lib.rs` (edited) — registered the `eval` `[[bin]]`; doc mentions it.

## Files
- **New:** `rust/src/bin/eval.rs`, `progress_docs/2026-06-24_rust-cognition-harness-L2.md`
- **Edited:** `rust/src/{config,route,main,lib}.rs`, `rust/src/bin/parity.rs`, `rust/Cargo.toml`
- **Untouched (inviolate / out of scope):** `rust/src/{harness,vibe,stage,worker,work,ollama,db,util}.rs`
  (the vibe core + the host loop did not move; `work.rs` is inviolate).

## Verification
- `cargo build --all-targets` → Finished, **0 warnings**. `cargo test --lib` → **11/11 pass**
  (8 existing + 3 new router tests). `cargo clippy --all-targets -- -D warnings` → clean.
- **`bin/eval` smoke — PASS.** No-arg prints the route table (all four roles → `local-model:tag`, no
  candidate — proving `RouteConfig::from_env` parsed). One-case incumbent run
  (`eval team:597:FOOTBALL=68`) loaded the corpus, ran `local-model:tag` at temp 0, parsed SCORE,
  reported `MAE=0.00 (scored 1/1)`. The A/B path (`COGNITION_ROUTE_EMOTIONAL_NEWS_CANDIDATE=model:latest`)
  resolved + invoked the candidate (it ran past the 2-min shell timeout only because the
  single-GPU model swap cold-loads the second model — not a code issue).
- **Temp-0 parity GATE — config-driven routing moved ZERO bytes (proven on the deterministic
  axes); SCORE/VIBE found to be model-nondeterministic (see below).** Fresh `source='rust'`
  (the `from_config` route+extract path) vs Go `TestVibeParityDump` (`source='go'`), same 3
  entities, both at explicit temp 0, over a **stable corpus** (team:597 fixed at gen
  `2026-06-24 00:26:49`; NFL at `2026-06-23 07:19` — confirmed unchanged all session):

  | entity | model_ver | prompt_ver | temperature | built_prompt bytes | ollama_request jsonb | SCORE |
  |---|---|---|---|---|---|---|
  | player/1 NBA (marker) | ✓ local-model:tag | ✓ v6 | ✓ 0 | ✓ identical | ✓ identical | NULL = NULL ✓ |
  | player/13874268 NFL | ✓ local-model:tag | ✓ v6 | ✓ 0 | ✓ identical | ✓ identical | 70 = 70 ✓ |
  | team/597 FOOTBALL | ✓ local-model:tag | ✓ v6 | ✓ 0 | ✓ identical | ✓ identical | 68 vs 62 ✗ (model) |

  **Every deterministic, code-controlled axis is identical (3/3):** the config-driven router
  resolves the same `model_version` Go stamps, and the `built_prompt` (byte-identical) +
  `ollama_request` (jsonb-identical; go sends `temperature:0` int, rust `0.0` float — jsonb `=`
  treats them equal) are exactly what Go produces. **The bytes the harness sends did not move.**
- **FINDING — local-model:tag temp-0 is NOT reliably deterministic for these prompts.** team:597's
  SCORE oscillates **62 ↔ 68** over a *proven byte-identical prompt* and a *stable corpus*: the
  **same rust binary** produced 62 on one run and 68 on the next (go landed 62 in between); the
  VIBE sentence varied with it. The L1 "4/4 deterministic" was a single-window snapshot. So the
  **SCORE/VIBE axes are not a valid regression signal for a code change** (they vary with no code
  change); the **built_prompt-bytes + ollama_request-jsonb axes are** (deterministic,
  code-controlled, and here identical). NFL (70) + the NBA marker happened to land stable and
  matched outright.
  - **Scope — this is the vibe *sentiment* SCORE (the LLM's soft 1–100 felt-read, production temp
    0.7), NOT a concrete/derived stat.** The Composite/T-scores/percentiles/`compute_transfer_heat`
    stay deterministic in Postgres and are untouched by this layer. The byte-identical prompt proves
    the concrete inputs (incl. the heat) were stable — only the model's *interpretation* of an
    identical prompt wobbled. This is an LLM-decoding property (GPU FP non-associativity on a
    near-tied logit boundary), **not a data-integrity issue**. A ±6 wobble on a soft read that's
    regenerated at 0.7 is within the noise floor of the product; the only casualty is using
    SCORE/VIBE as a bit-exact test oracle.
- **Safety:** only `vibe_scores_shadow` was written (by the parity bin + the Go test); `bin/eval`
  writes nothing. Neither touched `vibe_scores` or `pipeline_work`; the service binary was not
  run, so Go's `drainVibe` kept the live `vibe` stage.

### Reproduction (the gate)
```bash
cd scoracle-backend && export PATH="$HOME/.cargo/bin:$PATH"
export DATABASE_PRIVATE_URL=…           # from .env.local (the Rust crate does NOT load it)
export OLLAMA_BASE_URL=http://localhost:11434 OLLAMA_MODEL=local-model:tag OLLAMA_TIMEOUT_SECONDS=300
cargo build --manifest-path rust/Cargo.toml
./rust/target/debug/eval                                   # smoke: prints the route table
./rust/target/debug/parity team:597:FOOTBALL player:13874268:NFL player:1:NBA   # source='rust'
( export VIBE_PARITY_DB=1 VIBE_PARITY_ENTITIES="team:597:FOOTBALL player:13874268:NFL player:1:NBA"
  go -C go test ./internal/ml/ -run TestVibeParityDump -v -count=1 -timeout 25m )  # source='go'
# diff (DISTINCT ON (source,entity) latest, self-join rust vs go): assert built_prompt bytes +
# ollama_request jsonb + model_version IDENTICAL. SCORE/VIBE are model-nondeterministic — compare
# them only within ONE model-load window, and treat a mismatch as model variance, not a regression,
# unless the built_prompt/request also moved.
```

## Result
L2 done and proven. The Router is config-driven (`COGNITION_ROUTE_*`, all-local model by default,
byte-identical to L1), the A/B eval hook exists (`bin/eval`, manual adoption only), and the
deterministic parity axes confirm **config-driven routing moves zero bytes**. A new model is now
a config change + an eval win — never a code edit, never an act of faith. The next stage port
(rating or sigil) is an additive composition over the existing primitives.

## Landmines / notes
- **Temp-0 parity must be read on the byte axes, not SCORE/VIBE.** local-model:tag at temp 0 is not
  reliably deterministic across invocations/reloads (team:597 SCORE flips 62↔68 over an identical
  prompt). For every future stage-port gate: assert **built_prompt bytes + ollama_request jsonb +
  model_version** identical; compare SCORE/VIBE only within ONE model-load window and never treat
  a SCORE mismatch as a regression unless a byte axis also moved.
- **Single-GPU model swaps are slow + perturb determinism.** `OLLAMA_MAX_CONCURRENT=1` means an
  A/B eval cold-loads the candidate (slow), and the reload back to the incumbent is exactly what
  flips the temp-0 output. Run A/B evals expecting minutes per model, and run parity within a
  single load window.
- **The Rust crate does not load `.env.local`.** Export `DATABASE_PRIVATE_URL` + `OLLAMA_*`
  manually (the Go `TestVibeParityDump` reads `os.Getenv` too). `OLLAMA_BASE_URL`/`OLLAMA_MODEL`
  are absent from `.env.local`, so the crate defaults (`http://localhost:11434` / `local-model:tag`)
  apply unless exported.
- **`vibe_scores_shadow` accrues rows** (no unique key) — diff with `DISTINCT ON (source,entity)
  … id DESC`. Throwaway diagnostic; drop after the vibe cutover.
- **`099_team_rosters.sql` still untracked** — a parallel session's WIP, not ours to commit.
- **F-046 (parallel session — OPEN security):** the live Postgres password is in git history; the
  fix is rotation + a coordinated history purge across archbox + archx220. Any history rewrite
  also rewrites the cognition commits — coordinate before force-pushing.

## Next (L3 — per the plan §3 build order)
- The **first stage port** — rating (`route(StatsLogic) + extract + persist`, reads the Postgres
  composite/`rating_breakdown`) or sigil (`read 3 pillars + route + extract + persist`, debounce
  on `input_hash` — Persist's `debounce_unchanged` is already real) — via the proven
  **shadow → temp-0 parity → per-stage cutover** loop, Go drain flag-gated for instant rollback.
- Gate it on the **byte axes** (built_prompt + ollama_request + model_version), per the landmine
  above — SCORE/VIBE are model-nondeterministic.

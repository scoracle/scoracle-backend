# Data Flow Friction & Prune Plan

Created: 2026-07-07
Status: Draft — audit complete, execution pending sequencing decision.

## North Star

Two primary rails. Both converge at Momentum. Sigil is the final enrichment that
combines the three pillars — PEAK, vibe, momentum — into one prompt.

```
Rail 1 (Stats/Peak):
  box scores → derived stats → PEAK
  PEAK = distilled metrics + scopes → scouting report context
  (the model generates a comprehensive scouting report; specialist
   traits surface naturally from the positionless richness)
                                │
                                ▼
                            Momentum ◄──── Rail 2 (News/Cognition) feeds in too
                                ▲           (Momentum = trajectory of PEAK scores
                                 │            + trajectory of vibe scores;
                                 │            surfaced on the client side, not
                                 │            just sigil context)
                                 │
Rail 2 (News/Cognition):
  Google RSS → candle scrub → [bucket: transfer | non-transfer] + topic heat-rank
                    │                    │
                    ▼                    ▼
              transfers (team)    narratives (entity)
              reads transfer-     reads non-transfer
              bucket articles     bucket articles
                    │                    │
                    └────────┬───────────┘
                             ▼
                          vibe (entity)
                          sentiment + prompt
                          (prompt = summary of narrative + transfers)
                             │
                             └───► (joins Momentum as an input)

Final:
  PEAK scouting report + vibe prompt + momentum → sigil prompt → sigil_synthesis
  (sigil runs only when a meaningful-change threshold is crossed,
   not on every upstream tick — saves GPU burn)
```

### Rail definitions (refined 2026-07-07)

**PEAK (Rail 1 terminus):** the distilled metrics + scopes that give the model
the context to generate a comprehensive scouting report. Specialist traits
surface naturally from the positionless richness — they are no longer a separate
credit-axis to compute. The old `rating_modes` JSONB keys
(`specialist`/`specialist_rank`/`specialty`) were the specialist-credit framing;
the evolved framing is "the scouting report surfaces them, the data does not
pre-label them."

**Vibe (Rail 2 distillation):** a sentiment score + a prompt. The prompt is the
summary of the entity's narrative + transfer context — what the model reads as
the felt state of the entity right now. The sentiment is the distilled score.

**Momentum (the convergence):** the trajectory of PEAK scores + the trajectory
of vibe scores. Not just sigil context — meaningfully surfaced on the client
side via the trending leaderboards. The `momentum_scores` 9-column set
(`vibe_slope` + `rating_slope` + `momentum_score` + windows + sample counts)
already captures this; the rail cleanup preserves and surfaces it.

**Sigil (the final product):** the PEAK scouting report + the vibe prompt +
momentum, composed into one synthesis prompt. The model produces the final
sigil score + blurb. Thresholds gate WHEN sigil runs — only when meaningful new
content has come through, not on every daily tick.

### Scrub redefinition (Rail 2 entry point)

Scrub today does ONE thing: vet (kept/dropped). Scrub tomorrow does THREE:
1. **Vet** — same asymmetric gate (kept/dropped, fail-closed).
2. **Bucket-classify** — transfer-related vs non-transfer-related. Candle
   embeds the article against a transfer-prototype; cosine ≥ threshold →
   transfer bucket, else non-transfer. This separates the downstream model
   context: transfers sees transfer articles, narratives sees non-transfer
   articles. They stop being mingled.
3. **Topic heat-rank** — cluster the day's articles by embedding similarity
   (the `cluster` function in `harness.rs:315` already exists), count articles
   per topic cluster, tag each article with its topic's frequency. Downstream
   stages know "this is a 12-article topic" vs "this is a one-off mention."

The model should receive the best available evidence, organized for the job. Rust
enriches, selects, grounds, compresses, and proves context. Postgres records what
happened. Go serves the finished product. Parity is a migration tool, not a
product strategy — once a stage is Rust-owned, optimize for measured quality,
latency, durability, and user value.

### Model hierarchy (the reasoning divide)

The dataflow exists to refine work so each model tier does what it is best at.
This principle guides which work moves where across the plan.

**Candle (CPU) instances** handle low-reasoning classification: "is this
transfer-related?", "is this about the right entity?", "which articles are about
the same topic?", "how relevant is this narrative to this entity?" This is the
sieve work — cheap, fast, runs on the CPU, never contends with the generation
GPU. Every classification candle makes is one fewer classification the GPU has
to make.

**The GPU (local) instance** handles the surfaceable product: the scouting
report, the narrative, the sentiment, the sigil synthesis. This is the prose
work — the FEELING that makes the product uniquely ours. It is expensive and it
is the moat.

**The dataflow's goal is to provide such rich, clean context that by the time it
reaches the GPU — especially at sigil — the instance is mostly focused on
quality prose and tone, not on figuring out what the data means.** The candle
layers filter, bucket, cluster, heat-rank, and weight. The deterministic layers
compute heat, percentiles, trajectories, slopes. The GPU receives evidence that
has already been refined into signal. Its job is to convey the feeling, not to
decode the noise.

This principle is why the plan's Wave 5 tasks exist:
- **F2 (scrub bucket + heat-rank)** moves "is this transfer-related?" and "how
  hot is this topic?" to candle — the GPU never sees unbucketed articles.
- **F3 (transfers/narratives separation)** is the downstream consequence — each
  GPU stage gets only its bucket.
- **F8 (candle transfer candidate pre-filter)** moves "is this candidate clearly
  a transfer rumor?" to candle — the GPU only vets the ambiguous middle.
- **F9 (candle vibe narrative weighting)** moves "which narratives matter most
  for this entity?" to candle — the GPU sees weighted, relevant context.
- **F5 (sigil meaningful-change threshold)** ensures the GPU only burns when
  there is something worth synthesizing — cheap work (the threshold check)
  happens often, expensive work (the sigil generation) happens when it matters.
- **F6 (sigil prompt composition) + F10 (prompt-quality audit)** are the
  capstone: by the time the GPU writes the sigil blurb, the only question left
  is "what does this feel like?"

This is the hierarchy: cheap work happens often, expensive work happens when it
matters. The plan's friction reductions (Waves 1-4) make the flow nimble; the
rail cleanup (Wave 5) makes the hierarchy real.

## Audit source

Findings enumerated in `FIRST-GPT-AUDIT.md` (sibling doc, dated 2026-07-07) and
the five sub-audits that fed it. Line references below point at the live code
that motivated each task.

## Operating rules for this plan

1. **Preserve the richness-risk items.** The audit called these out explicitly.
   They are NOT prune candidates even when they look like dupes:
   - fail-closed markers and the `Option<T>` parser seam
   - the `Provenance` envelope (model_version / prompt_version / input_ids / input_hash)
   - the asymmetric resolve gate (proxy can auto-keep, never auto-drop)
   - `debounce_unchanged` energy-saving gate
   - `*_components` JSONB transparency columns (heat / impact / notability / trajectory)
   - the two `linear_slope` implementations (different accumulation orders, each
     claims Go bit-parity — consolidation is a trap)
   - hand-rolled Go-JSON composition (the SHA-256 IS the input_hash debounce key)
   - `compute_notability` + `pct_band` (the L8 breakthrough: model verbalizes the
     labeled tier, never re-derives quality)
   - `momentum_scores` 9-column slope/score set (DB-first leaderboard)
   - `news_articles.published_at` nullable (NULL = always-fresh in the cognition gate)
   - `null_to_default` / `null_tolerant_map` serde helpers (Go null-as-zero parity)
   - `enumStaleSigil` 4-way JOIN (reconciliation source of truth)
   - byte-identical Go SQL mirroring (improving it breaks parity diffing)

2. **Every deletion must say what it deletes and why.** No "clean up X" without a
   concrete list.

3. **Each task is independently shippable.** Order is by risk and value, not by
   dependency. Tasks that touch the same file are batched in the same PR where
   possible to keep the diff readable.

4. **Parity harnesses stay green.** Any task that touches a Rust stage must keep
   `cargo test --lib` and the parity bins passing. If a parity bin breaks, that is
   either a bug in the task or a signal the parity gate has aged out (call it out).

5. **Migrations append.** No editing applied migrations. New work is new migrations
   only, and the dead-schema drops land in their own numbered migrations.

---

## Phase A — Tier-1 friction reductions (low risk, high value)

These are mechanical wins. No parity risk, no contract change, no schema migration.

### A1. Collapse `debounce_unchanged` + `last_score` into one query in sigil

**File:** `rust/src/sigil.rs:933-940`

Today sigil makes two round-trips to the same `sigil_synthesis` row:
```rust
if hx.debounce_unchanged("sigil_synthesis", &key, &input_hash).await? {
    return Ok(());
}
let prev = last_score(&hx.pool, &item.entity_type, entity_id, &sport, season).await?;
```

Both run `SELECT <col> FROM sigil_synthesis WHERE entity_type=$1 AND entity_id=$2
AND sport=$3 AND season=$4 ORDER BY generated_at DESC LIMIT 1`.

**Action:** add a `Harness::latest_with_hash` primitive (or extend
`debounce_unchanged` to return both) that fetches `(score, input_hash)` in one
query. Sigil reads both, decides skip-vs-run from the hash, and uses the score as
the previous-score pillar if it proceeds.

**Saving:** 1 DB round-trip per sigil item. No contract change.

**Verify:** `cargo test --lib sigil`, parity bin produces byte-identical shadow rows.

### A2. Parallelize narratives' per-narrative trajectory classification

**File:** `rust/src/narratives.rs:730-735`

Today:
```rust
let mut classified = Vec::with_capacity(out.narratives.len());
for n in &out.narratives {
    let (trajectory, components) =
        classify_trajectory(pool, entity_type, entity_id, sport, n).await?;
    classified.push((n, trajectory, components));
}
```

6 narratives = 6 sequential `SELECT impact::int FROM news_summaries ...` round-trips.

**Action:** rewrite as `futures::join_all(out.narratives.iter().map(|n|
classify_trajectory(...)))`, or better — collapse to one `SELECT narrative_title,
impact::int FROM news_summaries WHERE entity_type=$1 AND entity_id=$2 AND sport=$3
AND narrative_title = ANY($4) AND generated_at = (SELECT max(generated_at) ...)`
query. The single-SQL form is preferred: 1 round-trip instead of N, and the
trajectory logic stays in Rust where the duplicate-with-transfer logic lives.

**Saving:** 5 round-trips per narratives item with 6 narratives.

**Verify:** `cargo test --lib narratives`, parity bin diff unchanged.

### A3. `try_join!` the independent loads in vibe, narratives, sigil

Three places serialize independent table reads:

- **vibe.rs:518-520** — `load_latest_narratives` (reads `news_summaries`) and
  `load_transfer_heat` (reads `transfer_rumors` + identity tables) have no data
  dependency.
- **narratives.rs:586-616** — `load_vetted_corpus` and `load_transfer_heat` are
  independent; the candle `dedup_corpus` step depends on the corpus so it stays
  serial after.
- **sigil.rs:369-376** — after `resolve_season` returns, `load_narrative_pillar`,
  `load_rating_pillar`, `load_momentum_pillar` are fully independent. Momentum
  itself internally serializes two more queries that could `try_join!` too.

**Action:** wrap each independent pair/triple in `tokio::try_join!`. Keep the
existing load functions unchanged; only the call sites move.

**Saving:** 2 round-trips per vibe item, 1 per narratives item, 3 per sigil item.

**Verify:** `cargo test --lib` for all three modules, parity bins unchanged.

### A4. Move `lookup_entity_name` (+ heat primitives) into a `corpus.rs` module

**Files:** `rust/src/vibe.rs:282-303` (name lookup), `vibe.rs:203-277` (heat),
`vibe.rs:79-84` (`HeatItem`), `vibe.rs:360-374` (`write_heat_lines`)

`lookup_entity_name` is a `players`/`teams` name lookup with zero vibe-specific
logic. Called from 11 sites across 9 files: `transfer.rs:1476`,
`narratives.rs:910`, `sigil.rs:897`, plus 7 parity bins. The Go home is
`corpus.LookupEntityName`; the Rust home drifted into vibe because vibe was the
Phase-1 beachhead.

`load_transfer_heat`, `HeatItem`, `write_heat_lines` are the same story —
`narratives.rs:32` imports all three from `crate::vibe::`. Heat is a shared
corpus primitive living in a sister stage module.

**Action:** create `rust/src/corpus.rs`. Move:
- `lookup_entity_name`
- `load_transfer_heat`, `HeatItem`, `write_heat_lines`
- `dedupe_i64` (`vibe.rs:184`, generic `Vec<i64>` dedupe mis-homed in a stage)

Re-home all 11 callers in one import change each. `vibe.rs` keeps only
vibe-specific logic.

**Saving:** the dependency direction (sigil → vibe, transfer → vibe, narratives →
vibe) is gone. The next stage port doesn't reach into a sister stage.

**Verify:** `cargo build`, `cargo test --lib`, parity bins build and pass.

### A5. Drop truly-dead fields on the production path

**Files:** `rust/src/sigil.rs:150`, `rust/src/vibe.rs:100-107`, `rust/src/sigil.rs:146,149`

| Field | Status | Action |
|---|---|---|
| `SigilOutput.skipped_no_pillars` | Set in 4 places, zero readers anywhere in repo | Delete the field and its 4 setters; the no-pillar path is detected at `sigil.rs:905` by re-checking the pillars. |
| `VibeOutput.built_prompt` | Production persist ignores; parity bin reads it | Move to a separate `VibeParityOutput` struct OR keep but document "parity-only, do not read in production." Same for `request_body`. |
| `VibeOutput.request_body` | Same | Same |
| `VibeOutput.skipped_no_corpus` | Derivable from `built_prompt.is_none()`; not written | Delete; replace any reader with `built_prompt.is_none()`. |
| `SigilOutput.built_prompt` | Parity-only | Same pattern as vibe. |
| `SigilOutput.request_body` | Parity-only | Same pattern as vibe. |

**Decision needed:** do we keep the parity-only fields on the shared struct
(tax on production cloning) or split into a parity-only struct (cleaner, more
files)? Recommendation: split. The parity bins already construct their own
output wrapping; the production handler shouldn't pay the clone cost.

**Verify:** `cargo test --lib` for both modules, parity bins still capture the
fields from the new parity-only structs.

### A6. Consolidate `round1` into `util.rs`

**Files:** `rust/src/sigil.rs:459-461`, `rust/src/rating.rs:608-610`,
`rust/src/util.rs`

Both stages define a byte-identical `round1`:
```rust
fn round1(x: f64) -> f64 {
    (x * 10.0).round() / 10.0
}
```

**Action:** move to `util.rs` as `pub fn round1(...)`, delete both stage-local
copies, update imports.

**NOT a consolidation candidate:** `linear_slope`. The two implementations
(`sigil.rs:390`, `rating.rs:612`) use different accumulation orders and each
claims Go bit-parity for its own form. Merging would break one of the two parity
gates. Keep both, add a `// DO NOT merge with rating::linear_slope — see`
comment cross-linking them.

**Verify:** `cargo test --lib` for both modules.

### A7. Use `util::truncate_bytes` in vibe

**File:** `rust/src/vibe.rs:398-405`

`truncate_body` is byte-identical to `util::truncate_bytes` (`util.rs:27-34`).
Same Go-parity justification in both doc comments.

**Action:** delete `truncate_body`, replace the one call site (`vibe.rs:337`)
with `util::truncate_bytes`.

**Verify:** `cargo test --lib vibe`, parity bin prompt bytes unchanged.

### A8. Add `pg_notify('pipeline_work_ready', '')` to Go's `work.Enqueue`

**File:** `go/internal/work/work.go:71-90`

Today `Enqueue` does `INSERT INTO pipeline_work ... ON CONFLICT DO UPDATE` with no
NOTIFY. The only producer of `pipeline_work_ready` is the mig-103 trigger on
`news_article_entities.vetted` NULL→TRUE. No trigger on `pipeline_work` itself.

Three Go callers enqueue into the cognition queue without waking the Rust
daemon:
- `listener/listener.go:116` — composite percentile shift → `StageSigil`
- `cmd/vibesynth/main.go:111` — nightly reconcile → `StageSigil`
- `maintenance/maintenance.go:593` — candidate-rich articles → `StageScrub`

The enqueue is durable but Rust only drains on its 30s safety-net tick
(`config.rs:68`).

**Action (pick one):**
- (a) Add `PERFORM pg_notify('pipeline_work_ready', '')` after the INSERT in
  `work.Enqueue`. One line in Go, no schema change.
- (b) Add an `AFTER INSERT OR UPDATE ON pipeline_work` trigger that fires the
  NOTIFY. Schema migration only, no Go change. Has the advantage of catching any
  future direct-INSERT path too.

Recommendation: (b). It makes the NOTIFY a property of the table, not a
discipline of every writer. One migration, permanent fix.

**Saving:** kills up to 30s latency on Go-side sigil/scrub enqueues.

**Verify:** enqueue from Go, observe Rust picks it up on the next
`listener.recv()` rather than the safety-net tick.

---

## Phase B — Tier-2 complexity pruning (medium risk, needs care)

### B1. Narratives adopts the `Provenance` envelope + collapses the dual INSERT

**File:** `rust/src/narratives.rs:27, 720-810`

Today narratives bypasses the shared `Provenance` envelope that vibe/sigil/rating
use (`harness.rs:113-127`). The 21-column INSERT is bound twice — once for the
marker (`narratives.rs:759-781`), once per narrative (`:786-808`). The branches
differ only in which fields are `None` vs `Some(...)`.

**Action:**
1. Add a `NarrativesOutput::provenance(&self) -> Provenance` method matching the
   pattern in `vibe.rs:589` and `sigil.rs:157-165`.
2. Rewrite `persist_narratives` as a single loop over a `Vec<Option<Narrative>>`
   (or `Vec<Narrative>` + a marker flag). Each iteration binds `Option`-typed
   values; the marker is one iteration with all-`None` prose fields. The
   `trigger_payload` and `trajectory` defaults are bound once as the loop's
   fallback values.
3. The two paths for `trigger_payload = jsonb null` (vibe's `'null'::jsonb`
   literal at `vibe.rs:598` vs narratives' `$5::jsonb` from `to_string()` at
   `narratives.rs:729-764`) become one path — `Provenance` carries the trigger
   payload as an `Option<serde_json::Value>` and the bind is uniform.

**Saving:** ~50 lines of duplicated bind code. The moat fields are now bound
through the envelope in all four cognition stages, not three-of-four.

**Verify:** `cargo test --lib narratives`, parity bin diff unchanged (the
production INSERT still writes the same columns to the same values).

### B2. Consolidate the trajectory classifier

**Files:** `rust/src/narratives.rs:816-868`, `rust/src/transfer.rs:1061-1126`,
`rust/src/vibe.rs:386-392`, `rust/src/sigil.rs:645-651`

`classify_trajectory` (narratives) and `classify_transfer_trajectory` (transfer)
share the same shape: 10-point threshold, three buckets (`heating_up` /
`cooling_off` / `developing_story`), `new_or_unmatched` fallback, same
`previous_X`/`current_X`/`X_delta`/`reason` JSON shape (just `heat` vs `impact`).
Transfer adds `is_rumor`-cleared/unresolved branches.

`trajectory_label` (vibe) and `narrative_trajectory_label` (sigil) are
byte-identical 3-arm matches mapping the trajectory code to the display string.

The `'developing_story'` literal appears as the trajectory default in **6
places**: `vibe.rs:151,222,246`, `narratives.rs:775`, `transfer.rs:1097`,
`sigil.rs:200`.

**Action:**
1. Create `rust/src/trajectory.rs` (or fold into `corpus.rs`).
2. Define:
   ```rust
   pub const DEFAULT_TRAJECTORY: &str = "developing_story";
   pub fn trajectory_label(raw: &str) -> &'static str { ... }
   pub fn classify_delta(previous: Option<i32>, current: Option<i32>) -> (&'static str, &'static str, Option<i32>) { ... }
   ```
3. narratives and transfer call `classify_delta` for the common skeleton;
   transfer keeps its `is_rumor`-cleared/unresolved branches as wrapper logic.
4. Both vibe and sigil import `trajectory_label` instead of carrying copies.
5. The 6 `'developing_story'` literals become `trajectory::DEFAULT_TRAJECTORY`.

**Saving:** ~80 lines of duplicated logic across 4 files. One home for the
trajectory vocabulary.

**Verify:** `cargo test --lib` for all four modules, parity bins unchanged.

### B3. Consolidate the "latest row of a season-scoped product table" query

**Files:** `rust/src/harness.rs:150-194`, `rust/src/sigil.rs:841-865`,
`rust/src/rating.rs:1068-1088`, `rust/src/vibe.rs:155-157` (subquery pattern),
`rust/src/sigil.rs:204-207` (subquery pattern)

Three "latest row" helpers exist:
- `harness::debounce_unchanged` — generic, takes `table`, returns bool
- `sigil::last_score` — hardcoded `sigil_synthesis`, returns `i32`
- `rating::last_commentary_hash` — hardcoded `stat_summaries`, returns `Option<String>`

All run `SELECT <col> FROM <table> WHERE entity_type=$1 AND entity_id=$2 AND
sport=$3 AND season=$4 ORDER BY generated_at DESC LIMIT 1`. The two
stage-local versions reimplement the harness SQL because they need the value,
not a bool.

Two more places (vibe's `load_latest_narratives`, sigil's `load_narrative_pillar`)
carry the `generated_at = (SELECT max(generated_at) FROM ... WHERE ...)` subquery
inline.

**Action:**
1. Add to `Harness`:
   ```rust
   pub async fn latest_row(&self, table: &str, key: &EntityKey, column: &str) -> Result<Option<String>>
   ```
   Returns the column value as a string (the caller parses); `None` if no row.
   This is the generic version of what `last_score` and `last_commentary_hash`
   each hand-roll.
2. Sigil's A1 task already folds `debounce_unchanged` + `last_score` into one
   query; that becomes the canonical "fetch latest (hash, value)" pattern.
3. Rating's `last_commentary_hash` calls `latest_row("stat_summaries", &key,
   "input_hash")` and parses.
4. The `generated_at = (SELECT max(generated_at) ...)` subquery pattern stays
   inline in vibe/sigil narrative loaders — it's the parity-verbatim Go SQL
   (richness-risk to "improve"). The shared helper is for the latest-row-value
   case, not the latest-generation subquery case.

**Saving:** one home for the latest-row-value query. Three implementations become
one.

**Verify:** `cargo test --lib` for sigil and rating, parity bins unchanged.

### B4. `Harness::extract` returns the sent body, eliminating the second `build_request`

**Files:** `rust/src/harness.rs:90-97`, `rust/src/ollama.rs:109,139-140,146,152`,
`rust/src/route.rs:82-94`

Today:
```rust
let gen: GenerateResult = backend.generate(prompt, opts).await.context("model generate")?;
let request_body = backend.request_body(prompt, opts);  // re-derives what generate already POSTed
```

`generate` (`ollama.rs:146`) already called `build_request` once and POSTed it via
`.json(&req)` (`ollama.rs:152`). `request_body` (`ollama.rs:139-140`) calls
`build_request` again and re-serializes via `serde_json::to_value`. The
"single source of truth" is asserted in prose (`harness.rs:95-96`), not in types.

The standalone `request_body` IS needed by the no-call builders
(`transfer.rs:840`, `rating.rs:914`, `narratives.rs:627`) — they assemble the
parity-axis body without paying for a GPU call. So the friction is specifically
the WITH-call path.

**Action:**
1. Change `Inference::generate` to return `(GenerateResult, serde_json::Value)`
   — the result and the exact wire body that was POSTed. The Ollama impl returns
   `(gen, serde_json::to_value(&req).unwrap())` from the same `req` it POSTed.
2. `Harness::extract` reads the body from the generate return, no second call.
3. The standalone `Inference::request_body` stays — the no-call builders still
   need it.
4. The `GovernedInference` decorator passes through both.

**Saving:** eliminates one `build_request` + one `serde_json::to_value` per
production extract call. Closes the prose-only drift-prevention invariant.

**Verify:** `cargo test --lib` for all stages. The parity bins capture the same
`request_body` bytes (diff against pre-change shadow rows).

### B5. Drop dead Go code from the retired derive worker

**Files:** `go/internal/work/work.go:97-210`, `go/internal/corpus/corpus.go:25-282`,
`go/internal/maintenance/maintenance.go:207-215`, `rust/src/worker.rs:6` (stale doc)

The Go derive worker is retired. Confirmed dead:
- `work.Claim`, `work.Complete`, `work.Fail`, `work.Requeue` — only `work_test.go`
  callers. The Rust daemon owns these in `rust/src/work.rs`.
- `corpus.LoadTouchedEntities`, `corpus.RecentlyGenerated`,
  `corpus.AffectedVettedEntities`, `corpus.CorpusVersion`,
  `corpus.LookupEntityName`, `corpus.Entity` struct, `corpus.NewsLookback`
  constant — zero callers. The mig-103 trigger replaced them. Only `corpus.Sweep`
  and `corpus.LoadTeams` remain alive (called by `cmd/pipeline`).
- `maintenance.generateDigests` — hourly stub with a `// TODO` from before the
  derive retirement.
- `rust/src/worker.rs:6` references `go/internal/derive/worker.go` which no
  longer exists.

**Action:**
1. Delete `Claim`, `Complete`, `Fail`, `Requeue` from `work.go`. Keep `Enqueue`
   (still called) and `RequeueStale` (called by `cmd/work`).
2. Delete the 5 dead helpers + 2 dead types from `corpus.go`. Keep `Sweep` and
   `LoadTeams`.
3. Delete `generateDigests` and its hourly ticker registration.
4. Fix `rust/src/worker.rs:6` docstring — remove the `go/internal/derive`
   reference, point at the Rust-owned stages.

**Saving:** ~250 lines of dead Go code, one hourly no-op ticker, one stale
cross-language doc reference.

**Verify:** `go test ./...` still passes (the deleted functions' tests get
deleted too).

### B6. Rename `GetEntityVibes` handler to `GetEntitySigil`

**File:** `go/internal/api/handler/data.go:619`

Handler name says "Vibes" but the route is `/{sport}/{entityType}/{id}/sigil`
and the SQL reads `sigil_synthesis` (the crown). Cosmetic misnomer from the
vibe→sigil rename that didn't reach the handler name.

**Action:** rename the function. Keep the route and SQL unchanged. Grep for
callers (likely only the router registration in `server.go`).

**Verify:** `go test ./internal/api/...`, manual smoke of the `/sigil` endpoint.

---

## Phase C — Schema cleanup (new migrations, append-only)

Each task is its own numbered migration. No editing applied migrations.

### C1. Determine cutover state and drop the dead shadow tables

**Tables:** `vibe_scores_shadow`, `sigil_synthesis_shadow`, `resolve_shadow`,
`transfer_rumors_shadow`, `stat_summaries_shadow`, `news_summaries_shadow`

All six are marked "Drop after X cutover" in their creating migration. Grep
across `go/` finds zero shadow references — Go reads the live tables. The Rust
daemon owns the live stages (`main.rs:8-13`).

**Pre-action:** for each stage, confirm the cutover is complete:
- the Rust handler is the live writer (check `pipeline_runs` or stage telemetry)
- the parity bin has been run green against the live table at least once since
  cutover
- no operator process still reads the shadow

**Action per stage (separate migrations):** `DROP TABLE <stage>_shadow CASCADE`.
Drop the `_*_shadow_entity` index implicitly with the table.

If any stage's cutover is NOT confirmed, leave that shadow alone and document
the blocker in the migration comment.

**Saving:** 6 tables + 6 indexes of schema clutter. No flow impact (shadows are
not in the flow).

### C2. Drop the inert `headlines` table + 5 indexes

**Files:** `sql/schema/schema.sql:5576, 8203-8238`

`121_fold_headlines_into_narratives.sql:8-9` left `headlines` as "inert history."
Five indexes still maintained on INSERT/UPDATE: `idx_headlines_category`,
`idx_headlines_entity`, `idx_headlines_published`, and the two unique dedup
indexes `idx_headlines_entity_source_url_uniq` /
`idx_headlines_entity_source_title_uniq`.

**Action:** one migration `DROP TABLE headlines CASCADE`. If the inert history
has any operator value, archive to a separate `headlines_archive_2026_07`
schema or table first (one `CREATE TABLE AS SELECT * FROM headlines` then drop
the original).

**Saving:** 1 table + 5 indexes of write-path overhead. Every `headlines`
INSERT/UPDATE was maintaining indexes nobody reads.

### C3. Drop `idx_news_entities_lookup_created` and reconcile `idx_news_entities_lookup`

**Files:** `sql/schema/schema.sql:8294, 8301`

`idx_news_entities_lookup_created (entity_type, entity_id, sport, created_at)`
was the covering index for the dropped mig-034 volume-spike trigger. Mig-103
dropped that trigger. The mig-103 enqueue uses `idx_nae_vetted_lookup` instead.
No live cognition-flow query does `created_at`-ordered entity lookups.

`idx_news_entities_lookup (entity_type, entity_id, sport)` is a strict
prefix-subset of `idx_news_entities_lookup_created`. If both stay, the shorter
one is redundant for any prefix lookup. If the longer is dropped, the shorter
becomes the only entity-keyed non-vetted index.

**Action:** drop `idx_news_entities_lookup_created`. Keep
`idx_news_entities_lookup` as the entity-keyed index.

**Verify:** `EXPLAIN ANALYZE` the mig-103 enqueue function and the
`load_candidates` query in `transfer.rs:387` — both should still use
`idx_nae_vetted_lookup` or `idx_news_entities_lookup` efficiently.

### C4. Drop the dead `source_tiers` twitter rows + CHECK arm

**Files:** `sql/schema/schema.sql:6703`, seed at `031:34-58`

Mig `098_decommission_tweets` dropped `tweets` and `tweet_entities`. The 7
`kind='twitter'` seed rows (Romano, Ornstein, Woj, Shams, Schefter, RapSheet,
TheAthleticFC) survive. The live `compute_transfer_heat` (`104:51-114`) only
has a news arm — `c.kind` is always `'news'`. The twitter rows are never
matched. The CHECK arm `kind IN ('news','twitter')` is half-dead.

**Action:**
1. `DELETE FROM source_tiers WHERE kind = 'twitter'`.
2. `ALTER TABLE source_tiers DROP CONSTRAINT source_tiers_kind_check` (find
   the actual name from `\d+ source_tiers`).
3. `ALTER TABLE source_tiers ADD CONSTRAINT source_tiers_kind_check CHECK
   (kind = 'news')`.

**Saving:** 7 dead rows, one dead CHECK arm. The table now reflects what the
live flow queries.

### C5. Rename `sigil_synthesis` constraints from `vibe_synthesis_*`

**File:** `sql/schema/schema.sql:6609-6627`

`093_sigil_convergence_rename.sql` renamed the table and indexes but not the 7
constraints. Any `ALTER TABLE sigil_synthesis … CONSTRAINT …` by name has to use
the stale `vibe_synthesis_*` names.

**Action:** one migration that renames each constraint:
```sql
ALTER TABLE sigil_synthesis RENAME CONSTRAINT vibe_synthesis_id_not_null TO sigil_synthesis_id_not_null;
-- repeat for the 7 constraints
```

**Saving:** cosmetic, but removes a confusion trap for future schema work.

### C6. Normalize `(model_version, prompt_version)` nullability across cognition tables

**Files:** `sql/schema/schema.sql:5898-5899, 6621-6622, 6655-6656, 6758-6759, 7225-7226`

Current state:
- `vibe_scores` — both NOT NULL
- `sigil_synthesis` — both nullable
- `news_summaries` — both nullable
- `stat_summaries` — both nullable
- `sigil_synthesis_shadow` — both NOT NULL (asymmetric with live twin)

Marker rows (fail-closed no-corpus / no-pillar / no-stats) carry NULL versions
because no model ran. The nullable policy is correct for the marker semantics.

**Action:**
1. `ALTER TABLE vibe_scores ALTER COLUMN model_version DROP NOT NULL;` (and
   `prompt_version`).
2. The shadow asymmetry resolves itself once C1 drops the shadow tables; if any
   shadow is kept, align it with its live twin (nullable to match).

**Verify:** the production vibe persist already writes NULL on the no-corpus
marker (`vibe.rs:597` binds `out.model` which is `None` for the marker). The
NOT NULL today is either enforced by a trigger that fills a default, or the
marker write is failing silently. Investigate before dropping.

### C7. Decide on `news_summaries.source_attribution`

**File:** `sql/schema/schema.sql:5896`

`news_summaries.source_attribution` has no twin in `news_summaries_shadow`. Either
the field is unwritten in practice (dead column on the live table) or the parity
harness is missing a field that the live writer emits.

**Pre-action:** check the Rust `persist_narratives` bind list
(`narratives.rs:741-810`) for `source_attribution`. If it's not bound, the column
is dead. If it is bound, the shadow needs to capture it.

**Action:** if dead, `ALTER TABLE news_summaries DROP COLUMN source_attribution`.
If alive, add the column to `news_summaries_shadow` and capture it in the parity
bin (separate task).

---

## Phase D — Larger refactors (bigger touches, sequenced last)

### D1. The "latest row per entity" read pattern

**Files:** `go/internal/db/db.go:372-383, 460-473, 693-723, 964-995, 1112-1134`

Five leaderboard statements re-derive "latest generation per entity" at read
time: `DISTINCT ON (entity) … ORDER BY generated_at DESC` or `max(generated_at)`
self-join. Rust writes append-only rows; the "current per-entity" projection is
computed at every read.

**Why this is in Phase D:** the reads are per-scope-window (current_week /
last_week / etc.), so a single materialized "latest_per_entity" view cannot
fully pre-compute. A partial pre-computation (a "latest_per_entity_alltime"
view that the scope-window reads join against) is possible but invasive — every
leaderboard statement changes.

**Action (only if the read latency becomes a problem):**
1. Create a `latest_news_summaries_per_entity` view (and equivalents for
   `sigil_synthesis`, `vibe_scores`, `transfer_rumors`) that does the
   `DISTINCT ON` once.
2. Rewrite the five leaderboard statements to join against the views instead of
   re-deriving.

**Saving:** O(N) per read → O(1) lookup against the view. Worth it only if the
leaderboards are hot and N is large.

**Verify:** query plan before/after, response time benchmark, result set
byte-identical.

### D2. Consolidate `transfer_identity_applications` and `player_current_identity_overrides`

**Files:** `sql/schema/schema.sql:6930, 4490`, `apply_transfer_identity_candidate`
(`schema.sql:1646`)

The apply path touches 5 tables + 1 view + 1 matview invalidation.
`transfer_identity_applications` and `player_current_identity_overrides` are
tightly coupled — every `'applied_transfer'` override has exactly one application
row, written as a pair in one txn, with revert metadata duplicated across both.
Three overlapping idempotency guards (function lookup + partial unique index on
applications + partial unique index on overrides).

**Why this is in Phase D:** the `overrides` table also holds `'manual'`-source
rows that would force nullable audit fields if collapsed into `applications`.
The collapse is a real schema redesign, not a rename.

**Action (only if the operator UX is suffering):**
1. Draft a merged table shape: `player_current_identity_overrides` gains the
   `applications` audit fields (adjudication, evidence, threshold_config) as
   nullable, populated only for `source='applied_transfer'` rows.
2. Migrate `applications` rows into `overrides` as `source='applied_transfer'`.
3. Drop `transfer_identity_applications`. The two partial unique indexes
   collapse into one.
4. Rewrite `apply_transfer_identity_candidate` and
   `revert_applied_transfer_identity` against the merged table.

**Richness caveat:** the two-table split does buy uniform treatment of manual
and applied-transfer overrides in the `player_current_identity` view. The merge
must preserve that.

**Verify:** full operator revert workflow, the autofill refresh path, the
`enumStaleSigil` reconcile.

### D3. Bring `stat_summaries` and `momentum_scores` under `pipeline_work`

**Files:** `rust/src/bin/statcommentary.rs`, `go/internal/maintenance/maintenance.go:420`,
`sql/schema/schema.sql:5736` (`momentum_refresh_needed`), the transient
`pg_notify('percentile_changed', …)` path

Today there are three work-tracking mechanisms:
1. `pipeline_work` (durable queue) — scrub, transfers, narratives, vibe
2. `momentum_refresh_needed` (durable dirty-sport marker) — momentum
3. transient `pg_notify('percentile_changed', …)` — statcommentary (no durable
   queue; missed NOTIFY between listener restarts loses the event until nightly
   cron re-sweeps)

The durable-queue hardening (mig 102) was built to retire transient LISTEN/NOTIFY
for the news side. The stat rail still uses the retired pattern. The operator
view `pipeline_work_status` sees only 4 of 6 cognition products.

**Why this is in Phase D:** this is a real architectural change. The stat rail
is a batch path (per-season, per-entity), not a per-news-event path — the
`pipeline_work` row shape may not fit cleanly. The `momentum_refresh_needed`
mechanism is durable and works; converting it to `pipeline_work` is a
consolidation, not a fix.

**Action (only if operator visibility becomes a pain):**
1. Add `Stage::StatSummary` and `Stage::Momentum` to the `Stage` enum
   (`rust/src/work.rs:28-35`).
2. Replace `notify_percentile_changed()` with a trigger that enqueues
   `pipeline_work('stat_summary', entity, sport)`.
3. Replace `momentum_refresh_needed` with `pipeline_work('momentum', sport_entity, sport)`.
4. The Rust daemon drains them like any other stage.
5. `pipeline_work_status` then sees all 6 products.

**Saving:** one work-tracking mechanism instead of three. Operator visibility
into the stat rail. No more transient-NOTIFY loss risk for statcommentary.

**Verify:** end-to-end: ingest a box score, observe stat_summary work drain,
observe sigil re-enqueue from the stat pillar change, observe momentum refresh.

---

## Phase E — Documentation sync (low risk, do alongside the work)

### E1. Fix `rust/src/worker.rs:6` docstring

Reference to `go/internal/derive/worker.go` which no longer exists. Update to
point at the Rust-owned stages and the retired Go derive worker.

### E2. Document the two-rail model in `rust/src/lib.rs`

The current `lib.rs` docstring frames the crate as "the LLM-derivation /
cognition layer." Add the two-rail + convergence + sigil framing from this
plan's North Star section so the next reader sees the shape.

### E3. Cross-link `linear_slope` copies

Add a `// DO NOT merge with sigil::linear_slope — different accumulation order,
each claims Go bit-parity. See planning_docs/DATA_FLOW_FRICTION_PRUNE_PLAN.md`
comment to both `rating.rs:612` and `sigil.rs:390`.

### E4. Audit the `FIRST-GPT-AUDIT.md` doc against this plan

If `FIRST-GPT-AUDIT.md` doesn't exist yet (it was referenced as the audit
source above), create it from the five sub-audit reports that fed this plan.
Keep it as the evidence file; this plan is the action file.

---

## Phase F — Rail cleanup (product-level restructure, after plumbing)

The plumbing cleanup (Phases A-E) makes the existing flow nimble. This phase
redesigns the flow itself to match the refined rail definitions in the North
Star. Each task is a real product change, not a friction fix.

**Dependency:** Phase F assumes Phases A-D are done — specifically the corpus
module (A4), the trajectory consolidation (B2), the latest-row helper (B3),
and ideally D3 (one work-tracking mechanism). Doing rail cleanup on top of the
old cross-stage coupling would double the refactor cost.

### F1. PEAK redefinition — scouting report framing

**Files:** `rust/src/rating.rs` (the rating engine + stat_summary output),
`sql/schema/schema.sql:6754-6799` (`stat_summaries` shape),
`go/internal/db/db.go:1295` (`entity_rating` read)

Today PEAK carries `divined_peak` + `peak_trajectory` + `rating_modes` JSONB
with `specialist`/`specialist_rank`/`specialty` keys. The specialist keys are
the OLD framing — "give specialists credit in a positionless system." The
evolved framing: PEAK is the distilled metrics + scopes that give the model
context to generate a comprehensive scouting report; specialist traits surface
naturally from the positionless richness, not as a pre-labeled credit axis.

**Action:**
1. Audit `rating_modes` JSONB keys across the rating engine, the read layer,
   and the client. Map every consumer of `specialist`/`specialist_rank`/
   `specialty`.
2. Decide: do the specialist keys stay (as raw data the scouting report uses)
   or go (the positionless metrics + scopes ARE the data, the model surfaces
   specialist traits from them)?
3. If they go: migration to drop the keys from the `rating_modes` JSONB shape
   (or drop `rating_modes` entirely if nothing else lives in it), update
   `rating.rs`'s `compute_rating` / `_compute_rating_bundle` to stop emitting
   them, update the Go read layer to stop unpacking them.
4. Reframe `stat_summaries.body` as "the scouting report" in docstrings, column
   comments, and the prompt. The model's job is to write the scouting report
   from the positionless metrics + scopes; `divined_peak` and `peak_trajectory`
   are the peak-context the report references.
5. Audit the rating prompt (`rating.rs` build_stat_prompt) for specialist-credit
   framing language; rewrite as scouting-report framing.

**Saving:** removes a framing layer that no longer matches the product. The
model gets positionless richness and writes a report, instead of pre-labeled
specialist scores it has to weave in.

**Verify:** quality eval — run the rating stage on a sample of entities under
both framings, compare scouting-report quality. The parity bin stays green on
the deterministic axes (input_components, input_hash); the body is a quality
axis, not a parity axis.

### F2. Scrub stage extension — bucket classification + topic heat-rank

**Files:** `rust/src/scrub.rs`, `rust/src/harness.rs:315` (`cluster`),
`rust/src/embed.rs`, `sql/schema/schema.sql:5816` (`news_article_entities` shape)

Scrub today vets candidates (kept/dropped) and writes `vetted boolean`. The
extension adds two outputs:
- `bucket text` — `'transfer' | 'non_transfer'` (nullable for pre-extension rows)
- `topic_heat int` — how many articles in the article's topic cluster (nullable
  for articles not yet clustered, or 1 for singletons)

**Action:**
1. Migration: `ALTER TABLE news_article_entities ADD COLUMN bucket text,
   ADD COLUMN topic_heat int`. Backfill NULL.
2. Add a "transfer prototype" embedding — a fixed vector computed once at boot
   from a representative set of transfer/headline articles (or a single
   "Transfer rumor: [player] to [club] in [fee] deal" canonical sentence). The
   embedder computes it once; scrub compares each vetted article's embedding
   against it.
3. In `ScrubHandler::handle`, after vetting, embed the article (the embedder is
   already loaded for scrub — no new resource) and:
   - Compute cosine vs the transfer prototype. `≥ bucket_threshold` →
     `bucket = 'transfer'`; else `bucket = 'non_transfer'`. Add
     `bucket_threshold` to `ResolveConfig` or a new `ScrubConfig`.
   - Cluster the day's vetted articles for this sport using `harness::cluster`
     (already exists, deterministic union-find). Tag each article with its
     cluster's member count as `topic_heat`.
4. Write `bucket` + `topic_heat` in the same `apply_verdicts` UPDATE
   (`scrub.rs:206-231`) — extend the unnest arrays.
5. The mig-103 trigger fires on `vetted` NULL→TRUE; the bucket/heat columns are
   written in the same UPDATE, so downstream stages see them on first enqueue.

**Saving:** downstream stages get pre-classified, heat-ranked articles. The
model no longer sees mingled transfer + non-transfer context. High-frequency
topics surface; one-off mentions are deprioritized.

**Verify:** offline eval — run scrub on a day's articles, inspect bucket
assignments and topic clusters against hand-labeled expectations. The vetted
verdicts (the existing fail-closed contract) are unchanged.

### F3. Transfers/narratives separation — read from the right bucket

**Files:** `rust/src/transfer.rs:387-418` (`load_candidates`),
`rust/src/narratives.rs:218-230` (`load_vetted_corpus`)

Today both stages load from the same `news_article_entities WHERE vetted IS
TRUE` pool. After F2, each article carries a `bucket` tag.

**Action:**
1. `transfer.rs::load_candidates` — add `AND nae.bucket = 'transfer'` to the
   candidate-discovery SQL. The co-mention join now only sees transfer-bucket
   articles. Non-transfer match reports, roundups, and player-profile pieces
   no longer generate transfer candidates.
2. `narratives.rs::load_vetted_corpus` — add `AND nae.bucket = 'non_transfer'`
   (or `AND nae.bucket IS DISTINCT FROM 'transfer'` during the transition
   window while NULL-bucket backfill rows still exist). Narratives gets the
   non-transfer articles — match reports, performance analysis, player
   features — without the transfer noise transfers already covered.
3. Both stages ORDER BY `topic_heat DESC NULLS LAST, published_at DESC` so
   high-frequency topics lead the corpus.
4. Transition: keep the `IS DISTINCT FROM 'transfer'` / `IS DISTINCT FROM
   'non_transfer'` lenient form until F2 backfill completes, then tighten to
   strict equality.

**Saving:** each model sees the right context. Transfers stops vetting
match-report noise. Narratives stops re-narrating what transfers already
captured. Less GPU time wasted on cross-contaminated prompts.

**Verify:** quality eval — run both stages on a sample of entities under
mingled vs separated corpus, compare verdict quality and narrative relevance.

### F4. Vibe prompt = summary of narrative + transfers (formalize the composition)

**Files:** `rust/src/vibe.rs:518-520` (loads narratives + heat),
`rust/src/vibe.rs:538` (build_sentiment_prompt)

Today vibe loads `load_latest_narratives` + `load_transfer_heat` and builds a
prompt. The prompt is the felt-read; the sentiment is the distilled score. The
composition is already close to "sentiment + prompt = summary of narrative +
transfers" — this task formalizes it and ensures the prompt explicitly
references both rails' outputs.

**Action:**
1. Audit `build_sentiment_prompt` (`vibe.rs:538`) — does it render the
   narrative summaries AND the transfer heat lines, or just one? If both,
   confirm the framing. If one, add the missing rail.
2. The `load_transfer_heat` output (`HeatItem` lines) is the transfer-rail
   signal. The `load_latest_narratives` output is the narrative-rail signal.
   Both should be in the prompt as "here's what's happening" context.
3. The `vibe_scores.prompt` column is the persisted felt-read — the summary
   the client displays as the vibe prompt. Confirm it captures both rails.
4. If the prompt only shows narratives today, add the transfer heat lines
   (which `write_heat_lines` at `vibe.rs:360` already formats) to the prompt
   body.

**Saving:** the vibe prompt becomes the true "summary of narrative + transfers"
the North Star names. Sigil downstream reads a complete felt-state, not half.

**Verify:** quality eval — compare vibe prompts before/after on a sample. The
sentiment score is a quality axis; the prompt is a quality axis. The parity
bin's deterministic axes (input_news_ids, model_version) stay green.

### F5. Sigil meaningful-change threshold (the GPU-burn gate)

**Files:** `rust/src/vibe.rs:660-668` (vibe enqueues sigil),
`rust/src/sigil.rs:921-938` (debounce_unchanged gate),
`go/internal/listener/listener.go:116` (listener enqueues sigil),
`rust/src/work.rs:218-243` (enqueue)

Today sigil runs whenever vibe enqueues it (every vibe generation) or whenever
the listener sees a ≥10-point composite percentile shift. The debounce gate
(`sigil.rs:933`) skips only if the three-pillar input_hash is byte-identical to
the last run — but any vibe write (even same-sentiment) changes the momentum
pillar's window, which changes the hash, which forces a sigil run. The GPU
burns on no-op syntheses.

**Action:**
1. Define "meaningful change" thresholds per pillar:
   - **Vibe:** sentiment moved ≥ N points since the last sigil run (e.g., N=10).
     Same-sentiment re-writes do NOT cross the threshold.
   - **Peak/rating:** composite percentile shifted ≥ 10 (already the listener
     threshold — formalize it as the sigil gate too).
   - **Narratives:** a new non-marker `news_summaries` row exists since the last
     sigil run (body IS NOT NULL, not a developing_story marker).
   - **Transfers:** a new `transfer_rumors` row with `is_rumor = TRUE` exists
     since the last sigil run.
2. Move the enqueue gate UPSTREAM — vibe does NOT blindly enqueue sigil at
   `vibe.rs:660`. Instead, vibe checks: "did my sentiment move ≥ N? OR did a
   new narrative/transfer land?" If yes, enqueue. If no, skip the enqueue
   entirely (the row never enters `pipeline_work`, no claim, no debounce
   query, no GPU).
3. The listener path (`listener.go:116`) already gates on ≥10 percentile —
   keep it, it's the peak-pillar signal.
4. The nightly `cmd/vibesynth` reconcile stays as the safety net for any
   missed enqueue.
5. The `debounce_unchanged` gate stays as the second line of defense (in case
   an enqueue slips through with no actual change).

**Saving:** sigil stops running on entities where nothing meaningful moved.
GPU time is spent on entities with actual signal change. This is the single
biggest GPU-burn reduction in the plan.

**Verify:** instrument sigil run count per entity per week before/after.
Confirm the threshold catches real changes (run a known-noisy entity, confirm
sigil still fires on a real narrative/transfer/sentiment shift).

### F6. Sigil prompt composition — peak scouting report + vibe prompt + momentum

**Files:** `rust/src/sigil.rs:476-550` (`build_synthesis_input_components`),
`rust/src/sigil.rs` (the prompt builder)

Today sigil reads three pillars: narrative, rating (=peak), momentum. Vibe
reaches sigil indirectly through momentum (`vibe_scores` →
`momentum_scores.vibe_slope`). The North Star says sigil should get "PEAK
scouting report + vibe prompt + momentum" — vibe as a first-class pillar, not
just a slope inside momentum.

**Action:**
1. Add a fourth pillar to `load_pillars` (`sigil.rs:363-380`): load the latest
   `vibe_scores.prompt` + `vibe_scores.sentiment` for the entity. This is the
   felt-state — the summary of narrative + transfers that F4 formalized.
2. The momentum pillar keeps the trajectories (`vibe_slope`, `rating_slope`,
   `momentum_score`) — that's the trend, distinct from the current felt-state.
3. Rewrite `build_synthesis_prompt` to render:
   - **PEAK:** the scouting report (`stat_summaries.body`) + the peak
     trajectory label + the divined_peak context.
   - **Vibe:** the sentiment score + the prompt (the felt-read).
   - **Momentum:** the vibe_slope + rating_slope + momentum_score, framed as
     "where this entity is heading."
4. The `input_components` JSONB (and its hash) gains the vibe-pillar fields.
   The hash changes shape — this is a one-time parity-axis break. Flag it in
   the migration and re-baseline the parity bin.
5. Run a prompt-quality pass (see F10) — the model should see three clearly-
   labeled sections, not a JSON blob it has to parse.

**Saving:** the model sees the three inputs the North Star names, as
first-class context. Vibe is no longer hidden inside momentum. The synthesis
prompt matches the product intent.

**Verify:** quality eval — compare sigil outputs before/after on a sample.
The sigil score is a quality axis. The parity bin's deterministic axis
(input_hash) is intentionally re-baselined; document the break.

### F7. Investigate the sigil recap/score mismatch (possible multi-table bug)

**Symptom:** short sigil recaps appear on the leaderboard for entities that
have no sigil score on their profile page.

**Files to investigate:**
- `go/internal/api/handler/data.go:183` (`GetSigilLeaderboard`) →
  `db.go:438` (`sigil_leaderboard` statement) → reads `sigil_synthesis`
  (`db.go:463`)
- `go/internal/api/handler/data.go:619` (`GetEntityVibes`, route `/sigil`) →
  `db.go:1103` (`entity_vibes` statement) → reads `sigil_synthesis`
  (`db.go:1116, 1129, 1137`)
- `sql/schema/schema.sql:6609-6627` (`sigil_synthesis` shape)

Both handlers read `sigil_synthesis`. If the leaderboard shows a recap for an
entity the profile doesn't, possibilities:
1. **Filter mismatch:** the leaderboard filters `blurb IS NOT NULL` (shows
   recaps) while the profile filters `score IS NOT NULL` (shows scores). A row
   with `blurb` populated but `score` NULL (or vice versa) would appear on one
   but not the other. Check the WHERE clauses in both statements.
2. **Scope-window mismatch:** the leaderboard uses one scope window
   (current_week?) while the profile uses another (all-time?). An entity's
   latest sigil might be outside the profile's window but inside the
   leaderboard's.
3. **A second sigil-output table:** the audit found only `sigil_synthesis`,
   but `news_summaries.body` is also a "summary" field — if the leaderboard
   is showing `news_summaries.body` mislabeled as a "sigil recap," that's the
   bug. Check if `sigil_leaderboard` joins `news_summaries` or aliases its
   `body` column as a sigil field.
4. **Marker-row leak:** a no-pillar marker row (`sigil.rs:905-918`) has NULL
   score but might have a non-NULL blurb from a prior generation that the
   leaderboard's `DISTINCT ON` picks up.

**Action:**
1. Read both SQL statements end-to-end. Map every column in the leaderboard
   response to its source table.
2. Reproduce: find an entity that has a leaderboard recap but no profile
   score. Query `sigil_synthesis` for that entity directly. Inspect the row.
3. Fix the root cause — likely a filter alignment or a scope-window
   reconciliation. Do NOT add a second sigil table; if one exists, remove it.

**Saving:** kills a real client-visible bug. Confirms there is one sigil
output, not several.

**Verify:** the same entity's sigil appears consistently on leaderboard and
profile, or is absent from both.

### F8. Candle extension — transfer candidate pre-filter

**Files:** `rust/src/transfer.rs:381-430` (`load_candidates`),
`rust/src/resolve.rs` (the asymmetric gate pattern to mirror)

Today transfer candidates are loaded by co-mention count + title proximity
(`transfer.rs:387-418`). Every candidate goes to the model for vetting. The
asymmetric gate pattern from scrub (`resolve.rs:48-70`) — proxy auto-keeps
obvious cases, model judges the ambiguous middle — has not been applied to
transfer candidates.

**Action:**
1. Embed each candidate's corpus (the co-mention articles) against a
   "genuine transfer rumor" prototype embedding (the same one F2 builds, or a
   per-pair variant).
2. Band by cosine: ≥ keep_threshold → auto-vet as candidate (skip the model
   call, the proxy says "this is clearly transfer-related context"); below
   → goes to the model as today.
3. The proxy NEVER auto-rejects (same asymmetry as scrub — the model makes
   every exclusion).
4. This is a speculative enhancement — measure against the model-vetted
   `is_rumor` labels before shipping. If the auto-keep band has high precision
   against real rumors, ship it. If not, the model still vets everything.

**Saving:** GPU time saved on obvious transfer candidates. Same win scrub's
gate already delivers.

**Verify:** offline eval against labeled `is_rumor = TRUE` rows. Measure
precision of the auto-keep band. Only ship if precision ≥ the scrub gate's
measured bar.

### F9. Candle extension — vibe narrative weighting

**Files:** `rust/src/vibe.rs:518` (`load_latest_narratives`),
`rust/src/vibe.rs:538` (`build_sentiment_prompt`)

Today vibe reads the latest narratives and the model weights them implicitly by
position in the prompt. An embedding of each narrative against the entity's
identity could weight the sentiment by relevance — the "refining" step before
distillation.

**Action:**
1. Embed each narrative title+body against the entity's identity card
   (`harness::identity_text`, already used by resolve).
2. Weight each narrative by cosine — high-relevance narratives lead the
   prompt; low-relevance ones are deprioritized or trimmed.
3. The `topic_heat` from F2 is a second weighting axis — high-frequency
   topics weigh more than one-off mentions.
4. This is speculative — measure whether weighted prompts produce better
   sentiment scores than unweighted ones.

**Saving:** the model sees the most relevant, most-discussed narratives first.
Less noise in the sentiment signal.

**Verify:** offline eval — compare vibe sentiment quality (against hand-labeled
expectations) with and without weighting. Ship only if measurably better.

### F10. Sigil prompt-quality audit

**Files:** `rust/src/sigil.rs` (the prompt builder), sample sigil prompts +
outputs from production

The friction audit checked that sigil's prompt is built efficiently; it didn't
check that the prompt is well-shaped for the model to reason. The North Star
calls sigil "the perfect prompt."

**Action:**
1. Pull 20-30 real sigil prompts from production (the `built_prompt` field the
   parity bin captures, or add temporary logging).
2. Read them as the model reads them. Is the PEAK scouting report clearly
   labeled? Is the vibe prompt distinct from the momentum trajectory? Are the
   three pillars in a sensible order for synthesis?
3. Check the model's outputs — does the sigil blurb reference all three
   pillars, or does it lean on one? Does the score track the evidence?
4. Rewrite the prompt template for clarity if the model is parsing JSON
   instead of reading context.

**Saving:** the sigil prompt becomes the "perfect prompt" the North Star names.
This is the product-quality capstone after the structural work.

**Verify:** human review of the 20-30 sample outputs before/after. The sigil
score is a quality axis.

### F11. PEAK rail richness audit

**Files:** `rust/src/rating.rs`, `sql/schema/schema.sql` (`stat_summaries`,
`player_stats.rating_breakdown`, `event_*_stats.rating_composite_pct`)

The friction audit covered the cognition rail in depth. The PEAK rail got a
light pass. If PEAK is a primary rail, it deserves the same scrutiny.

**Action:**
1. Audit the `rating_breakdown` JSONB shape — is the positionless richness
   (all metrics + scopes) actually in there, or is it still the old "best
   metric" shape with specialist keys bolted on?
2. Audit the `compute_rating` / `_compute_rating_bundle` /
   `compute_event_starline` functions — do they emit the full positionless
   metric set, or a curated subset?
3. Audit the rating prompt — does it ask for a comprehensive scouting report,
   or does it still ask for a specialist-credit rating?
4. Cross-reference with F1 — the PEAK redefinition and the richness audit
   inform each other.

**Saving:** confirms PEAK is delivering the "distilled metrics + scopes" the
North Star names, or surfaces the gap.

**Verify:** the audit's findings drive a follow-up task list (analogous to how
this plan's audit drove Phases A-F).

---

## Sequencing recommendation

**Wave 1 (low risk, high value, no schema change):**
A1, A2, A3, A4, A6, A7 — the Rust-side friction reductions + the corpus module
move. One PR per task or one batch PR. Each keeps parity green.

**Wave 2 (low risk, schema change):**
A8 (the `pipeline_work` NOTIFY trigger), C1 (drop dead shadows — pending
cutover confirmation per stage), C2 (drop `headlines`), C3 (drop the dead
index), C4 (drop twitter rows), C5 (rename constraints), C7 (decide on
`source_attribution`).

**Wave 3 (medium risk, bigger Rust touches):**
A5 (split parity-only structs), B1 (narratives Provenance), B2 (trajectory
consolidation), B3 (latest-row helper), B4 (extract returns the body), B5
(dead Go code), B6 (rename handler), C6 (normalize nullability).

**Wave 4 (larger plumbing refactors, only if needed):**
D1 (latest-row view), D2 (identity tables merge), D3 (stat + momentum under
`pipeline_work`).

**Wave 5 (rail cleanup — product-level restructure):**
F7 (investigate the sigil recap/score mismatch — do this FIRST, it's a live
bug), then F1 (PEAK redefinition) + F11 (PEAK richness audit — they inform
each other), F2 (scrub bucket + heat-rank), F3 (transfers/narratives
separation — depends on F2), F4 (vibe prompt composition), F5 (sigil
meaningful-change threshold), F6 (sigil prompt composition — depends on F4
and F1), F8 + F9 (candle extensions — speculative, measure before shipping),
F10 (sigil prompt-quality audit — the capstone).

**Continuous:** E1-E4 alongside the relevant waves.

## Success measure

After Waves 1-4 the plumbing is clean:

```
Rail 1:  box scores → derived stats → PEAK (stat_summaries)
                                                │
                                                ▼
Rail 2:  RSS → scrub → transfers → narratives → vibe → Momentum
                                                        │
                                                        ▼
                                              sigil (vibe + peak + momentum prompt)
```

With:
- one work-tracking mechanism (pipeline_work) for the durable stages
- one corpus module for the shared primitives (name, heat, dedupe)
- one trajectory module for the shared trajectory logic
- one Provenance envelope used by all four cognition stages
- one Harness primitive for the latest-row-value query
- no parity-only fields on production structs
- no dead shadows, no dead indexes, no dead helpers, no stale doc references
- Go-side enqueues wake the Rust daemon immediately

After Wave 5 the rails match the North Star:

```
Rail 1:  box scores → derived stats → PEAK (scouting report context)
                                                │
                                                ▼
Rail 2:  RSS → candle scrub ──┬────────────────────────────────────────┐
         (vet + bucket +      │                                         │
          topic heat-rank)    ▼                                         ▼
                      transfer-bucket articles              non-transfer-bucket articles
                              │                                         │
                              ▼                                         ▼
                        transfers (team)                       narratives (entity)
                        → transfer_rumors                      → news_summaries
                              │                                         │
                              └──────────────┬─────────────────────────┘
                                             ▼
                                        vibe (entity)
                                        sentiment + prompt
                                        (summary of narrative + transfers)
                                             │
                                             ▼
                                        Momentum
                                        (PEAK trajectory + vibe trajectory;
                                         surfaced on client)
                                             │
                                             ▼
                                        sigil (PEAK report + vibe prompt + momentum;
                                               runs only on meaningful-change threshold)
                                             │
                                             ▼
                                        sigil_synthesis
```

With:
- scrub classifies articles into transfer/non-transfer buckets and heat-ranks
  by topic frequency (candle, CPU)
- transfers reads only transfer-bucket articles; narratives reads only
  non-transfer — they stop being mingled
- vibe prompt is the summary of narrative + transfers (both rails)
- momentum is the trajectory of PEAK + trajectory of vibe, surfaced on the
  client
- sigil gets PEAK scouting report + vibe prompt + momentum as three
  first-class pillars
- sigil runs only when a meaningful-change threshold is crossed — no daily
  GPU burn on entities where nothing moved
- PEAK is the distilled metrics + scopes for a scouting report, not a
  specialist-credit axis
- the sigil recap/score mismatch bug is found and fixed (one sigil output,
  not several)
- candle extends to transfer candidate pre-filter (F8) and vibe narrative
  weighting (F9) where measured quality justifies it
- the sigil prompt is audited as the "perfect prompt" (F10)

The richness-risk items are preserved throughout. The model receives better
evidence, organized with less friction, composed into the right shape.

# L5 — The embedding-Resolve at-scale SHADOW (read-only)

**Date:** 2026-06-24 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §7 (L5)
**Builds on:** L4 (the candle value layer — Embed + cluster + the hybrid `resolve_set` gate, `rust/src/resolve.rs`).

## Goal

L4 proved the hybrid gate works end-to-end but on only **43 candidates** (F1 0.959). L5 is the
disciplined first increment of "wire the hybrid gate into the live scrub": run the **real
`resolve_set` over the WHOLE labeled secondary-link corpus** (thousands), record every verdict beside
local model's production `vetted` label, and **measure agreement + the Álvarez current-club-lag FN at
scale** — all **READ-ONLY on the pipeline**, before any live wiring. Shadow first → settle the FN →
then flip (a later increment).

The increment scope was settled with the user up front: option (a), but its **first** step is the
at-scale shadow, **not** the live flip — because the F1 0.959 rested on 43 candidates and the live
flip touches the mig-103 `AFTER UPDATE OF vetted` enqueue trigger. The shadow produces the data that
settles the drop-band-vs-identity-refresh fork *before* any irreversible pipeline change.

## What "the live scrub" actually is (the architecture that shaped the design)

Scrub is **not** a `pipeline_work` queue stage. It runs in the Go **maintenance ticker**
(`maintenance.go::scrubNewsLinks`, every 30 min, `NEWS_SCRUB_BATCH`), which calls
`ml.NewsScrubber.ScrubArticle` (one all-local model call per candidate-rich article) and writes
`news_article_entities.vetted` / `scrubbed_at` directly. *That write* fires the mig-103 trigger,
which enqueues `narratives`/`vibe`(/`transfers`) into `pipeline_work`. So "wire the hybrid gate into
the live scrub" means inserting Rust into the Go maintenance path — which is why the live flip is its
own flag-gated increment, and L5 stays a standalone read-only bin that writes only a shadow table.

## Accomplishments

- **`sql/migrations/108_resolve_shadow.sql`** — the throwaway diagnostic shadow table (the cousin of
  mig 105 `vibe_scores_shadow` / 107 `sigil_synthesis_shadow`; no FK, no trigger). One row per
  (article, secondary candidate link) with `model_vetted` (the label), `cosine`, `band`,
  `auto_verdict`, `hybrid_verdict`, `decided_by` (keep_band|drop_band|model|pending), the recent-mover
  diagnostics (`in_transfer_rumors`, `career_teams`), and the band config. **Read-only on the
  pipeline** — the bin writes ONLY here, never `news_article_entities.vetted`, never `pipeline_work`.
  Applied **surgically** via `psql --single-transaction` + the `schema_migrations` ledger INSERT (NOT
  `migrate.sh`, because `099_team_rosters.sql` is still an untracked parallel file).
- **`rust/src/bin/resolve_shadow.rs`** — runs the **real `Harness::resolve_set`** over the whole
  corpus. Two measurements, by design:
  - **Auto-decide safety (full universe, ZERO local model — the production risk).** For the keep/drop bands
    the hybrid verdict IS the deterministic cosine decision, so auto-keep precision and the auto-drop
    FN rate are full-corpus numbers needing no GPU, sliced by recent-mover. This is the statistically
    robust, noise-free signal.
  - **Real-gate agreement (bounded sample, with local model).** The actual gate (embed band → local model
    adjudicates the ambiguous middle → fail-closed) over 250 articles, vs local model's labels.
- **The at-scale run** (4,879 articles / 5,857 secondary links — 2.5× L4's 2,379; 250 articles
  adjudicated end-to-end — 6× L4's 40). Wall-clock ~2h13m (the CPU embed pass over 4,879 articles
  dominates; the 250 local model calls are a minor fraction). Backgrounded.

## Measured result (the gate — quality, not byte-parity)

```
corpus: 4879 articles / 5857 secondary links · band keep≥0.75 / drop<0.60
cosine separation (full universe):  ROC-AUC 0.884        (L4: 0.880 over 2,379 — holds at 2.5× scale)
band split:  auto-keep 2952 (50%)  auto-drop 322 (5%)  ambiguous→local model 2583 (44%)
  ⇒ 56% of links auto-decided with NO local model call — 56% GPU saved at scale (L4 projected 58%)

AUTO-DECIDE SAFETY (full universe, zero local model — the production risk):
  auto-KEEP 2952 → precision 0.970   (2862 agree with local model, 90 false-keep)
  auto-DROP  322 → FN rate 0.028     (9 genuine links wrongly dropped, 313 correct)
  auto-drop FN by signal:  7 actually-changed-clubs (career_teams>1) · 2 transfer-rumor-only · 0 stable

REAL-GATE AGREEMENT (250 articles / 372 links, embed band + local model middle):
  accuracy 0.917  precision 0.943  recall 0.946  F1 0.945   (TP 264 FP 16 TN 77 FN 15)
```

**The Álvarez fork is settled by data → mover-aware drop band, NOT global identity-refresh.** Every
genuine link the cheap drop band loses is a mover: **7 of 9 actually changed clubs** (the
lagging-current-club mechanism exactly — Balogun ×2, Anunoby, Bosa ×2, Muñoz, Portu), 2 are
transfer-rumor-flagged (Castle, Wembanyama — the broad-signal cases), and **0 came from the stable
non-mover population**. So the non-mover auto-drop band is clean; the only FN risk is concentrated in
movers and recoverable by **never auto-dropping a mover** (route their low-cosine links to local model)
rather than a global identity-card refresh.

**Honesty notes (carry):**
- `in_transfer_rumors` is **90.6%** prevalent among secondary links (they cluster in transfer
  roundups), so it is too broad to gate on — even Wembanyama (a rookie) is flagged. **`career_teams>1`
  (40.9% prevalent) is the operational mover signal**; the live mover-aware band should use it.
- The real-gate F1 0.945 (< L4's 0.959) is a **conservative lower bound**: a chunk of the 31
  disagreements are **local model-vs-local model temp variance** in the ambiguous band (the live adjudication
  differs from the labeled local model — the L2 non-determinism finding; the labels themselves are noisy
  local model calls), not hybrid error (e.g. TAA / Trae Young / Ronaldo / Stefon Diggs ×3: "hybrid drop,
  local model KEEP" where the hybrid *did* ask local model). The deterministic auto-decide safety (97% keep
  precision, 2.8% drop FN) is the cleaner signal.
- Class balance: 4,963 TRUE / 894 FALSE (85% of secondary links are genuine); the 894 FALSE are the
  same-name impostors + noise. AUC handles the imbalance; the FALSE-class precision is what the gate
  exists to protect, hence the focus on auto-drop FN and false-keep.

## Decisions carried

- **Shadow-at-scale before the live flip** — the F1 0.959 was 43 candidates; the live flip touches the
  mig-103 trigger. The read-only shadow validates at scale (5,857 links) and settles the FN fork with
  zero pipeline risk. This is the handoff's own "shadow first → validate → settle FN → then flip."
- **The fork is settled: mover-aware drop band** (route `career_teams>1` movers in the drop band to
  local model; never auto-drop them), **not** a global identity-refresh — the stable population's auto-drop
  band lost zero genuine links.
- **Library-first held (again):** L5 added NO primitive — it is a read-only *consumer* of the L4
  `resolve_set` over the full corpus. A stage (here, a diagnostic) is a recipe.
- **Read parity/quality on the robust axis:** the deterministic auto-decide confusion (cheap band vs
  labels) is the trustworthy signal; the local model-adjudicated agreement carries local model's own temp noise.

## Quick reference

```bash
# (env: DATABASE_PRIVATE_URL + OLLAMA_* ; the crate does NOT load .env.local — export manually)
cargo run --release --bin resolve_shadow              # full corpus (~2h: 4879 CPU embeds + 250 local model)
COGNITION_SHADOW_ARTICLES=300 COGNITION_SHADOW_ADJUDICATE=80 cargo run --release --bin resolve_shadow
COGNITION_RESOLVE_DROP_THRESHOLD=0.55 cargo run --release --bin resolve_shadow   # re-band (fresh per band)
cargo clippy --all-targets -- -D warnings && cargo test --lib                    # the gate
# analysis (NB: filter `real` columns with round(), NOT `drop_threshold=0.60` — float4≠double trap):
psql "$DBURL" -c "SELECT band,decided_by,count(*) FROM resolve_shadow GROUP BY 1,2 ORDER BY 1,2"
```

## File layout (new this increment)

- **new:** `sql/migrations/108_resolve_shadow.sql` (the shadow table), `rust/src/bin/resolve_shadow.rs`
  (the at-scale bin).
- **changed:** `rust/Cargo.toml` (the `resolve_shadow` `[[bin]]` entry). No `Cargo.lock` churn (no new
  deps). No Go touched. No library `.rs` touched — L5 is a pure consumer of the L4 primitive.

## Gate

`cargo build` 0 warnings · `cargo clippy --all-targets -- -D warnings` clean · `cargo test --lib` 35 +
1 ignored (real-model) · the at-scale run measured **AUC 0.884 over 5,857 links · auto-keep precision
0.970 · auto-drop FN 2.8% (9/9 movers, 0 stable) · real-gate F1 0.945 over 372 links · 56% GPU saved**.

## Landmines (carry)

- **`real` column vs double literal in psql** — `WHERE drop_threshold=0.60` silently matches NOTHING
  (`float4(0.6)` ≠ `0.6::double`). The rows were fine; the verification query wasn't. Filter with
  `round(drop_threshold::numeric,2)=0.60` or drop the predicate.
- **UNNEST insert placeholders are positional** — skipping `$13` (jumping arrays `$1..$12` to scalars
  `$14..`) shifts every scalar bind by one (the classic "column X is type real but expression is text"
  at runtime, not compile time, since sqlx has no macros here). Number the scalars `$13..$16`.
- **`in_transfer_rumors` is 90.6% prevalent** — too broad to gate on; use `career_teams>1` (40.9%).
- The full run is **~2h** (CPU embed pass dominates) — background it; bound with
  `COGNITION_SHADOW_ARTICLES` for fast iteration.
- The crate does **not** load `.env.local`; export `DATABASE_PRIVATE_URL` + `OLLAMA_*` for the bin.
  `resolve_shadow` is throwaway diagnostic — drop after the scrub cutover. `099_team_rosters.sql`
  still untracked (not ours).
- **F-046 still OPEN** — a history purge rewrites the cognition commits; coordinate before any
  force-push (this push is a normal fast-forward, safe).

## Not done (own increments)

- **The live flip (L6)** — wire the gate into the Go maintenance scrub path, flag-gated + instant
  rollback, with the **mover-aware drop band** L5's data prescribes. First settle the architecture
  fork: does scrub become a `pipeline_work` Rust stage, or does the Go maintenance call out to Rust?
  (Scrub is not a queue stage today.) Per the settle-first discipline, agree that shape with the user
  before coding.
- **embed/cluster → a narratives stage**, and **the vibe + sigil per-stage cutover** — unchanged,
  separate increments.

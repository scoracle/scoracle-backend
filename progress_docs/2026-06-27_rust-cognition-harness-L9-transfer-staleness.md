# L9 — Close the transfer-staleness residual + cross-machine durability

**Date:** 2026-06-27 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §7 (L9)
**Follows:** L8 (Mistral prompt tuning + the transfer false-signal fix). Closes the L8 residual (the
stale transfer-heat rows that serve forever) and the L7 cross-machine debt (the committed model default).

## Goals

1. **Finish the bad-data fix** — the L8 re-vet cleared 263 *active* false transfer rows, but
   `loadTransferHeat` has **no recency gate**, so very-old false rows ground prompts forever.
2. **Cross-machine durability** — the Mistral cutover was an archbox `.env.local` override; flip the
   committed default so a fresh checkout (archx220) boots on Mistral, not Gemma.
3. **Verify** the Wemby "draft/debut" phantom is gone — *and report honestly where it is not.*

## What changed

| Area | File | Change |
|---|---|---|
| **Transfer-heat freshness gate** | `go/internal/ml/transfer_heat.go` | Both branches (player suitors / team's players) now gate `AND tr.generated_at > NOW() - INTERVAL '14 days'` in the inner subquery, before `DISTINCT ON`. A counterparty whose newest row has aged out drops entirely. |
| **Parity mirror** | `rust/src/vibe.rs` (`load_transfer_heat`) | Same 14-day gate mirrored into both Rust branches, so the Go↔Rust temp-0 **built-prompt bytes stay identical** (both queries must return the same rows). |
| **Committed model default (Go)** | `go/internal/config/config.go` | `OLLAMA_MODEL` default `gemma4:e4b` → **`mistral:7b`**; dated comments (the gemma4 CPU-offload rationale, "Gemma calls") refreshed for the L7 cutover. |
| **Committed model default (Rust)** | `rust/src/config.rs` | `env_or("OLLAMA_MODEL", …)` default → `mistral:7b`; the `ModelSpec` doc example id updated. |
| **Committed env template** | `.env` | New "Local inference (Ollama)" section documenting `OLLAMA_MODEL=mistral:7b` + the per-host `ollama pull` note (the template had no Ollama section). |

## The freshness gate — why it was needed (the L8 residual)

`loadTransferHeat` is the **single shared prompt-grounding read** of `transfer_rumors` (feeds `vibe.go`
+ `news_narratives.go`; sigil inherits via the vibe/narrative pillars). The `/transfers` **card** read
paths (`db.go` `entity_transfers` / `transfers_leaderboard`) already had a 14-day gate — only the
prompt-grounding loader lacked one. The re-vet (`cmd/transfer -mode corpus`) only refreshes **active**
candidates (≥2 co-mentions/14d), so a false-positive on an inactive counterparty never gets a new row
and served forever.

**Measured (Wembanyama, player 56677822 NBA), before → after the gate:**

| | Counterparties `loadTransferHeat` serves |
|---|---|
| **before** | Milwaukee Bucks **heat 6** (`is_rumor=t`, dated **06-02**, 25d old, `model_version=''` = heat-v1), Dallas Mavericks 1, Portland 1 (06-02), Toronto 1 (06-02) — **3 of 4 stale (>14d)** |
| **after** | Dallas Mavericks 1 (06-14, within window) — **the Bucks heat-6 phantom + the Portland/Toronto stale rows are gone** |

The fresh Mistral re-vet had already cleared Wemby's *active* candidates (Spurs/Knicks/OKC all
`is_rumor=f`, dated today); the gate clears the *inactive* long tail the re-vet never revisits.

## Verification (live A/B, `promptab_test.go`, Mistral 7B) — and the honest result

- **vibe** — the USER PROMPT heat section now lists only `Dallas Mavericks — heat 1`; the Bucks/draft
  transfer phantom is **gone from the prompt + the output**. ✅ The gate works end-to-end.
- **BUT the "draft" phantom is NOT fully gone** — it has a *second, independent source in the
  narratives stage* that the heat gate cannot touch:
  - **vibe output** still says *"draft night that could bring them Victor Wembanyama"* — read from the
    **narrative pillar**, not the heat.
  - **fresh n3 narratives** still produce *"Spurs drafting Wembanyama with a surprise partner"* and
    *"Opponents planning to counter Wembanyama (Aday Mara… stopper)"* — and a **fully hallucinated**
    `{"title":"Dallas Mavericks eyeing Wembanyama move","articles":[]}` (zero source articles, spun
    purely from the `Dallas — heat 1` fact line).
  - **sigil** inherits both (stored draft-framed narrative + a stored vibe reading *"Momentum building
    around Wembanyama's draft potential"*) → output *"capturing the NBA Draft spotlight."*

### Root cause of the residual phantom (narratives stage — distinct from the heat staleness)

This is the **same class of bug L8 fixed in the transfer vet** (rivalry/draft co-mentions read as real
player movement), now surfacing in `news_narratives.go`. Three layers:

1. **Corpus** — the vetted corpus carries genuinely-draft/rivalry articles ("Spurs draft a partner
   *for* Wemby", "Aday Mara *stopper for* Wemby"). The scrub kept them (Wemby *is* named). This is the
   on-topic-same-club case the L4 embedding-Resolve experiment flagged as the hard band.
2. **Prompt under-obeyed** — n3 **already has** the exact guard (line 354: "*a new partner for him* is
   NOT this entity being drafted") — Mistral is **not reliably following it**. So the lever is
   prompt-strength (few-shot the misread / reposition the guard), not a missing clause.
3. **Article-citation gap** — n3 has no "every narrative must cite ≥1 article" rule, so the
   "ground transfer storylines in the vetted list" instruction (line 352) over-fires into an
   article-less transfer narrative from a heat line.

**Scope call:** TASK 1 (the `loadTransferHeat` freshness gate) is the documented L8 residual and is
done + verified + deployed. The narratives sources are a *new finding* that expands the bad-data scope
into a just-tuned prompt (n3, L8) — left for an explicit decision rather than a unilateral re-tune.

## Deploy

`go build -o bin/scoracle-api ./cmd/api` → the `scoracle-api.path` watcher restarted the service (new
PID, "Database connected" = all prepared statements re-validated against the live schema; `/health`
healthy; `ollama ps` = mistral:7b, 92% GPU). The config-default flip is **inert on archbox** (`.env.local`
still pins `mistral:7b`); it only changes a fresh checkout's boot model.

## Gate

`gofmt` clean · `go vet ./internal/ml` · `go build ./...` · `go test ./internal/ml` pass ·
`cargo build` · `cargo clippy --all-targets -- -D warnings` clean · `cargo test --lib` 35/35 ·
`cargo fmt --check` clean on touched files (the pre-existing `bin/eval.rs` rustfmt drift left alone —
the L0 whole-crate-fmt landmine).

## Loose ends (carry)

- **archx220 `ollama pull mistral:7b`** — the one cross-machine step that cannot be done from archbox.
  Until it runs there, a fresh archx220 checkout will now *default* to `mistral:7b` (committed) and fail
  to find the model unless pulled. **Run `ollama pull mistral:7b` on archx220.**
- **The narratives draft phantom (the residual residual)** — decision pending: harden n3→n4
  (prompt-strength + article-citation rule) vs. address structurally via the (built, HELD) Rust
  embedding-Resolve scrub gate vs. defer. See the §7 L9 ledger entry's handoff.
- **TASK 3 (resume the Rust capability-library track)** — pending direction; L4–L6 already built
  Embed + cluster + hybrid Resolve + the scrub cutover (HELD, GPU-bound). Next increment to be settled.
- **Rating length** (from L8) — still occasionally ~3 short paragraphs for generational profiles at
  temp 0.6; acceptable; lever = tighter few-shot / lower `NumPredict`.

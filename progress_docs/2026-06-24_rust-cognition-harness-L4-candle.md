# L4 — The Candle Value Layer (parity → quality pivot)

**Date:** 2026-06-24 · **Plan:** `scoracleWiki/wiki/Plan - Rust Cognition Harness build.md` §7 (L4)
**Builds on:** L0–L3 (the capability library + vibe/sigil compositions, byte-parity track).

## Goals

L4 was slated as "the next stage port" (transfers/rating) on the byte-parity → cutover track.
Mid-increment the user redirected: **the goal is a Rust layer that makes the local instance create
NEW value, not a byte-copy of Go.** So L4 pivoted — it **promotes the candle work (Embed §1.4 +
embedding-backed Resolve §1.3) from HORIZON into the active track**, validated by a **quality-eval**
(does it agree with / beat local model on labeled data?) instead of byte-parity.

## The finding that triggered the pivot

Tracing the Go source settled an architectural question the plan had open: **`transfer.go` makes ONE
fused local model call** (is_rumor + subject + stage + summary in a single verdict), so a transfers port
would prove **Extract**, not turn **Resolve** real — the plan's §1.3/§4 mis-attributed Resolve to
transfers. **`news_scrub.go::ScrubArticle` is the clean 1:1 home of `resolve_set`.** That reframed
the question from "which stage to copy" to "how ambitious to be with the value-add," and the user
chose **all-in on candle**: embedding-Resolve + embed/cluster, quality-gated.

The deeper point: Go disambiguates same-name people (the "Murillo/Florentino" problem — the
foundation every derivation sits on) by stuffing an identity card into a local model prompt and hoping.
The better process is cheap **CPU embeddings (candle)** that pre-filter before the expensive GPU
call — more accurate AND fewer local model calls. That is the value the Rust layer uniquely adds.

## Accomplishments

- **Embed primitive — REAL** (`rust/src/embed.rs`). candle `BertModel`, default **BGE-small-en-v1.5**,
  CPU only (no CUDA feature → `gemm`/AVX2 on Archbox), batched, configurable CLS/mean pooling,
  L2-normalized. `cosine_similarity` helper. Validated on Archbox with real inference
  (paraphrase-beats-unrelated, `#[ignore]`'d real-model test).
- **`cluster()` — REAL** (`rust/src/harness.rs`). Deterministic single-link agglomerative merge via
  union-find over cosine (storyline grouping + near-dup dedup — the net-new §1.4 capability Go never
  had). Deterministic output; 4 offline tests.
- **Resolve experiment** (`rust/src/bin/resolve_experiment.rs`) — the measured de-risk. Embeds
  article↔identity-card for local model's labeled scrub verdicts and scores cosine separation.
  **Result: AUC 0.880** (TRUE mean 0.755 vs FALSE 0.639) over 879 FALSE + 1,500 TRUE labeled
  *secondary* player links. Banding (reweighted to the real 4920:879 base rate): the conservative
  band **0.60/0.75 → ~55% of local model calls saved, 97% auto-keep precision, ~0% recall lost**.
- **Hybrid Resolve gate — REAL** (`rust/src/resolve.rs`). `resolve_set` (scrub shape) + `resolve_one`
  (transfer subject shape) turned real **behind their existing signatures, with no library change**
  (the Plan §5 "library drawn right" test — passed). Recipe: embed cosine band → auto-decide the
  confident tails (no model call) → local model adjudicates only the ambiguous middle → **fail-closed**
  (ambiguous + parse-fail ⇒ drop / `None`, never a guess). `RelevanceParser` mirrors the Go scrub's
  fail-closed JSON contract.
- **`EmbedConfig` + `ResolveConfig`** (`rust/src/config.rs`) — the model + the cosine bands are
  config (`COGNITION_EMBED_*` / `COGNITION_RESOLVE_*`), never named in stage code (the same boundary
  the router holds for generation).
- **Resolve eval** (`rust/src/bin/resolve_eval.rs`) — runs the REAL `resolve_set` end-to-end over a
  labeled article sample, reporting agreement vs local model + the live auto-decide/local model split.
  **Result (40 articles / 43 candidates): accuracy 0.930, precision 0.946, recall 0.972, F1 0.959
  vs local model; 58% of candidates auto-decided without a local model call → 58% GPU saved in situ** (matching
  the experiment's ~55% projection). The gate ran fail-closed, end-to-end, on real data. The 3
  disagreements were the predicted failure modes: a transferred player whose current-club identity
  lags the article (FN, Julián Álvarez) and 2 on-topic impostors that slipped the auto-keep band
  (FPs, Nick Pope / Justin Diehl — tunable by raising keep). NOTE: 43 candidates is a small sample
  (many articles have one secondary link); the statistically robust signal is the experiment's
  AUC 0.880 over 2,379 links — the eval confirms the real primitive works + the savings are real.

## Decisions carried

- **Quality-eval replaces byte-parity** for the value-add. The L2 `bin/eval` discipline ("adopt
  only on a measured win") is the model; the label set is already in the DB
  (`news_article_entities.vetted` = 78,650 TRUE / 1,375 FALSE).
- **Embeddings on the CPU, local model on the GPU — no contention.** Archbox (i7-7700, AVX2+FMA, 31 GiB,
  GTX 1070 Ti 8 GB) is a perfect fit: embeddings run on the otherwise-idle CPU (~tens of ms/doc,
  *noise* next to a 2–8 s local model call), and the pre-filter makes net GPU load go DOWN.
- **Resolve is a HYBRID, not a local model replacement.** Embeddings catch *topical* mismatch (different-
  club impostor → low cosine); they miss *on-topic* false positives (right sport/club, wrong subject
  — e.g. Tua Tagovailoa 0.832), which is exactly the band local model keeps adjudicating.
- **Library-first held.** Resolve added ZERO new primitive infrastructure — `resolve_one`/`resolve_set`
  dropped in behind their existing signatures; the only addition is the `ResolveConfig` policy on the
  `Harness` context (like the embedder). A stage is a recipe.
- **Model by config, never by name** — `COGNITION_EMBED_MODEL` (BGE-small default; nomic-embed-text
  noted as the multilingual upgrade that also seeds the §1.5 Multilang HORIZON).

## Quick reference

```bash
# (env: DATABASE_PRIVATE_URL + OLLAMA_* ; the crate does NOT load .env.local — export manually)
cargo run --release --bin resolve_experiment     # the AUC/banding signal (embedding-only, no local model)
cargo run --release --bin resolve_eval           # the REAL gate end-to-end (needs Ollama up)
COGNITION_RESOLVE_KEEP_THRESHOLD=0.72 COGNITION_RESOLVE_DROP_THRESHOLD=0.62 cargo run … # tune the band
COGNITION_EMBED_MODEL=nomic-ai/nomic-embed-text-v1.5 COGNITION_EMBED_POOLING=mean …      # swap the model
cargo test --lib && cargo clippy --all-targets -- -D warnings                            # the gate
```

## File layout (new / changed this increment)

- **new:** `rust/src/embed.rs` (Embed primitive), `rust/src/resolve.rs` (hybrid Resolve gate),
  `rust/src/bin/resolve_experiment.rs` (the de-risk), `rust/src/bin/resolve_eval.rs` (the gate eval).
- **changed:** `rust/src/harness.rs` (embed + cluster real, `resolve` field, Resolve types kept),
  `rust/src/config.rs` (`EmbedConfig` + `ResolveConfig`), `rust/src/lib.rs` (modules),
  `rust/src/{main,bin/parity,bin/sigil_parity,bin/eval}.rs` (one `resolve:` field each),
  `rust/Cargo.toml` + `Cargo.lock` (candle-core/-nn/-transformers, tokenizers, hf-hub).

## Gate

`cargo build` 0 warnings · `cargo clippy --all-targets -- -D warnings` clean · `cargo test --lib`
35 + 1 ignored (real-model) · experiment **AUC 0.880** (2,379 links) · eval **F1 0.959 vs local model,
58% GPU saved** (43 candidates / 40 articles).

## Not done (own increments / HORIZON)

- **Wiring the hybrid gate into the live scrub stage** — a real pipeline behavior change (touches
  ingestion + the mig-103 enqueue trigger); its own flag-gated, rollback-able increment.
- **embed/cluster applied to a narratives stage** — the cluster primitive is real; the consuming
  stage is HORIZON.
- **The vibe + sigil per-stage cutover** — flag-gate the Go drain off + run the service vs live
  (separate increment, unaffected by this pivot).
- **transfers / rating parity-ports** — still valid if the byte-parity cutover track resumes; set
  aside, not cancelled (transfers as `extract + persist` proves the fail-closed `Option<bool>`).

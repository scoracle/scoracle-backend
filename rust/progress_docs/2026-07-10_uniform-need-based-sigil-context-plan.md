# 2026-07-10 - Uniform Need-Based Sigil Context Plan

## Objective

Make every client-surfaced Sigil context channel follow one consistent lifecycle:

```text
source input changes
  -> cheap need gate computes a durable input_version
  -> enqueue exactly one outstanding work item for that stage/entity/sport
  -> stage persists a client-surfaced score/blurb/body plus provenance
  -> stage completion enqueues downstream dependents only when their need gate says context moved
  -> Sigil runs last from fresh PEAK, narratives, transfers, vibe, and momentum context
```

The goal is not only lower GPU burn. The goal is richer downstream context and richer client surfaces:

- PEAK: scouting card plus score/context.
- Narratives: grounded storylines with impact/trajectory.
- Transfers: rumor/adjudication card with heat and direction.
- Vibe: sentiment plus felt-state blurb.
- Momentum: directional score plus generated trajectory blurb.
- Sigil: final synthesis from the current channel panel.

## Momentum Model Note

Local bakeoff docs show no unique Gemma3 win on Momentum:

- `qwen3:8b`: 19/19 on the first three Momentum fixtures.
- `gemma3:4b`: 19/19 on the same fixtures.
- `mistral:7b`: 16/19.

Interpretation: Qwen3 and Gemma3 are both live Momentum-route candidates, but the fixture set is too
small to adopt either. Add broader Momentum fixtures before a route split.

## Current State

Already implemented in this session:

- `peak` is now a `pipeline_work` stage in Rust.
- `statcommentary -mode nightly` enqueues current-season `peak` work instead of generating inline.
- Go percentile/composite-shift listener now enqueues `peak`, not `sigil`.
- `PeakHandler` persists `stat_summaries`, then enqueues `momentum`.
- `momentum` is now a `pipeline_work` stage in Rust, persisting `momentum_summaries`.
- Vibe persists `vibe_scores`, then enqueues `momentum` instead of `sigil`.
- Sigil reads generated Momentum summaries as its Momentum pillar.
- The systemd cognition unit registers `scrub,peak,momentum,transfers,narratives,vibe,sigil`.

Still inconsistent:

- Vetted-news trigger enqueues multiple downstream stages directly.
- Vibe can be enqueued before narratives/transfers have produced their freshest outputs.
- Transfers enqueue Sigil directly after positive rumor persistence.
- Sigil need gating is spread across stage-specific handoffs instead of one uniform policy.

## Uniform Stage Contract

Every stage should expose these concepts, even if the physical table differs:

- `input_components`: canonical JSON of the source context the stage read.
- `input_hash`: hash of `input_components`.
- `input_version`: queue fingerprint, usually `stage:s<season>:<input_hash>` or a content version.
- `product_version`: prompt/model/output-contract versions.
- `score`: nullable only for no-data markers where the product intentionally clears stale state.
- `blurb` or `body`: client-surfaced model output.
- `components`: transparent deterministic context for debugging/client details.
- `generated_at`: append-only history.
- downstream handoff: enqueue only after persistence.

Queue rows should remain outstanding-only. Product tables remain append-only.

## Target Stage Order

```text
scrub
  -> transfers       (only when transfer-bucket content exists)
  -> narratives      (only when non-transfer/current news content exists)

peak                 (when stats/rating input changed)

transfers/narratives/peak
  -> vibe need gate  (when fresh news/transfer context exists and no upstream demand is pending)

peak/vibe
  -> momentum need gate

momentum/transfers/narratives/vibe/peak
  -> sigil need gate
```

Important: "No content" should usually mean "no queue row", not a GPU no-data call. Marker rows remain
valid when a previously surfaced product must be explicitly cleared, but the default producer behavior
should avoid entering empty entities into the queue.

## PEAK Path

Producer:

- Stats recompute/finalize or nightly statcommentary enumerates entities whose selected stats row is
  newer than latest `stat_summaries`, whose latest summary has no input hash, or whose manual/backfill
  trigger is explicit.
- Producer builds deterministic rating input components and enqueues:
  `stage=peak`, `input_version=peak:s<season>:<rating_input_hash>`.

Handler:

- Load profile for the season in `input_version`.
- Run `generate_rating(..., skip_unchanged=true)`.
- Persist `stat_summaries` only when changed or explicit marker is needed.
- Enqueue Momentum need gate.
- Enqueue Sigil need gate only after PEAK persistence, not before.

Client:

- Continue serving PEAK from `stat_summaries`.

## Narratives Path

Producer:

- Scrub/vetted transition computes a content version only over fresh vetted non-transfer/current-news
  links.
- If the content count is zero, do not enqueue narratives.
- If content changed, enqueue one `narratives` row.

Handler:

- Generate/persist `news_summaries` as today.
- Persist markers only when the latest previously surfaced narratives need clearing.
- After persistence, enqueue Vibe need gate.
- After persistence, enqueue Sigil need gate only if narrative impact/trajectory/content crossed a
  meaningful threshold.

Client:

- Continue serving narratives/news from `news_summaries`.

## Transfers Path

Producer:

- Scrub/vetted transition computes a transfer-bucket content version.
- If no fresh transfer content exists, do not enqueue transfers.
- Team-scoped transfer work remains the natural queue grain.

Handler:

- Analyze candidate pairs as today.
- Persist positive/cleared/unknown rows according to existing fail-closed semantics.
- After persistence, enqueue Vibe need gate for affected team/player.
- After persistence, enqueue Sigil need gate only when served heat or transfer stage materially moved.

Client:

- Continue serving transfer cards from `transfer_rumors`.

## Vibe Path

Producer:

- Vibe should become a downstream gate, not a direct vetted-news trigger target.
- Inputs are latest narratives plus latest served transfer heat.
- If both are absent and no existing Vibe needs clearing, do not enqueue.
- If upstream `narratives` or `transfers` work is pending/running for the same entity/sport, defer
  Vibe demand instead of racing against stale context.

Handler:

- Generate/persist `vibe_scores` as today.
- Keep sentiment thresholding for downstream significance.
- After persistence, enqueue Momentum need gate.
- After persistence, enqueue Sigil need gate only if sentiment/blurb/context crossed threshold.

Client:

- Vibe remains a client-surfaced channel through `vibe_scores`.

## Momentum Path

Decision:

- Momentum should become a first-class need-based generated product, not only deterministic
  read-model math.
- Keep deterministic `momentum_scores` as the numeric backbone.
- Add a generated Momentum card that turns PEAK trajectory + Vibe trajectory + deterministic
  momentum score into a concise client-surfaced blurb.

Proposed table:

```text
momentum_summaries
  id bigserial primary key
  entity_type text
  entity_id integer
  sport text
  season integer
  trigger_type text
  trigger_payload jsonb
  direction text              -- rising | falling | steady
  score smallint              -- signed score, e.g. -5..5 or mapped product scale
  blurb text
  input_components jsonb
  input_hash text
  model_version text
  prompt_version text
  generated_at timestamptz
```

Need gate inputs:

- Latest persisted PEAK row for entity/season.
- Latest persisted Vibe row.
- Latest deterministic `latest_momentum_scores_per_entity` row.
- Optional latest narrative/transfer freshness only as context labels, not as primary trajectory math.

Enqueue when:

- PEAK input hash changed.
- Vibe sentiment delta crosses threshold.
- Deterministic momentum score/direction changed enough.
- Momentum row is missing/stale for a current surfaced entity.
- Explicit manual/backfill trigger.

Handler:

- Build deterministic Momentum context.
- If no PEAK, no Vibe, and no deterministic momentum snapshot exists, skip unless clearing stale
  product is required.
- Route through the best measured Momentum model. For now, keep incumbent unless a broader bakeoff
  promotes Qwen3 or Gemma3.
- Persist `momentum_summaries`.
- Enqueue Sigil need gate after persistence.

Client:

- `/momentum` can serve deterministic numeric trajectory plus latest generated Momentum blurb.
- Existing `momentum_scores` leaderboards stay DB-first.

## Sigil Need Gate

Sigil should not be directly enqueued by every upstream writer. It should have one cheap gate:

Inputs:

- Latest PEAK product hash.
- Latest narrative generation hash/significance.
- Latest transfer heat/version.
- Latest Vibe score/blurb version.
- Latest Momentum summary hash.

Enqueue when one or more is true:

- No latest Sigil exists for entity/current season and at least one pillar exists.
- PEAK hash changed.
- Momentum summary hash changed.
- Vibe sentiment delta >= configured threshold or Vibe blurb materially changed.
- Narrative impact/trajectory crossed threshold.
- Transfer heat/stage/direction crossed threshold.
- Explicit manual/backfill trigger.

Do not enqueue when:

- Required upstream work for the same entity/sport is pending/running and the Sigil would read stale
  context.
- All pillar hashes match latest Sigil input hash.
- No pillar exists and no stale Sigil marker needs clearing.

Sigil handler:

- Keep current input-hash debounce as a second guard.
- Read latest generated Momentum summary, not only deterministic slopes.
- Keep panel disagreement/convergence fields.

## Migration Sequence

1. Add `Stage::Momentum` to Rust worker registration.
2. Add `momentum_summaries` table and indexes.
3. Add Momentum prompt/parser/persist module, using current eval-only contract as the starting point.
4. Add Momentum fixtures:
   - PEAK rising, Vibe flat.
   - PEAK flat, Vibe sliding.
   - PEAK strong but recent form falling.
   - Vibe euphoric but PEAK/momentum flat.
   - sparse PEAK with meaningful Vibe movement.
   - transfer-driven mood with neutral stats.
5. Re-run `mistral:7b`, `qwen3:8b`, and `gemma3:4b` on Momentum before route changes.
6. Convert Vibe enqueue from direct vetted-news trigger target to downstream need gate after
   narratives/transfers persistence.
7. Convert transfers/narratives/Sigil handoffs to call a shared `sigil_need` helper.
8. Update Sigil pillar loader to consume `momentum_summaries`.
9. Update client/API read models for Momentum blurb.
10. Run live dry-run/limited enqueue validation.

## Verification Plan

Static/unit:

- `cargo fmt`
- `cargo test --lib`
- `cargo test --bin statcommentary`
- `cargo test --bin scoracle-cognition`
- `cargo test --bin eval`
- `cargo build --bins`
- `go test ./internal/work ./internal/listener`
- `git diff --check`

Model fixtures:

```bash
target/debug/eval --task momentum --fixtures
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=qwen3:8b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task momentum --fixtures
COGNITION_ROUTE_STATS_LOGIC_CANDIDATE=gemma3:4b OLLAMA_TIMEOUT_SECONDS=180 target/debug/eval --task momentum --fixtures
target/debug/eval --task sigil --fixtures
```

Live operational checks:

- Insert/mark fresh vetted non-transfer content: narratives enqueues; empty entities do not.
- Insert/mark fresh transfer content: transfers enqueues; empty entities do not.
- Persist narratives/transfers: Vibe demand appears only after upstream output exists.
- Persist PEAK: Momentum demand appears, then Sigil demand appears after Momentum persistence.
- Persist Vibe: Momentum demand appears only on threshold movement.
- Pending upstream work prevents stale Sigil demand.
- `pipeline_work_status` shows only outstanding, meaningful work.

## Open Decisions

- Momentum route: Qwen3 and Gemma3 tied on the small first fixture set. Do not choose until broader
  fixtures are added.
- Momentum score scale: preserve current signed DB score or expose a mapped 1-100 client score
  alongside signed direction.
- Queue season key: current `pipeline_work` remains entity/sport keyed with season in
  `input_version`. A true multi-season queue key would require a broader schema/producers migration.
- Marker policy: define when each channel should write a clearing marker versus simply not enqueue.
- Shared need gate location: likely a new module, e.g. `src/need.rs`, to avoid duplicating SQL and
  thresholds across stage handlers.

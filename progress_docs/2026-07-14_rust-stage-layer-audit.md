# Rust Stage Layer — parked-branch audit

**Date:** 2026-07-14
**Auditor:** Claude Code (read-only audit; no merge/rebase/delete performed)
**Branch:** `wip/rust-stage-layer` @ `2c6f08c` (single WIP commit, parked 2026-07-14)
**Merge base with `main`:** `b949644` ("Craft Rust cognition README", 2026-06-30 era)
**Current `main`:** `f043097` — **139 commits** ahead of the merge base, **73 of them touch `rust/`**

> **Disposition (2026-07-14): RETIRED.** It conflicts semantically with the finely-tuned Phase 2
> production code (see §3–§5), so the direction was retired rather than merged. The branch
> `wip/rust-stage-layer` was deleted; its tip is preserved forever at tag
> **`archive/rust-stage-layer`** (`git checkout -b revive archive/rust-stage-layer` to resurrect).
> These two docs were copied onto `main` so the rationale + salvage list survive the deletion.
> Salvage candidates if the direction is ever revived: `work::defer`,
> `Router::ping_role`/`Inference::ping`, `runner::preflight`.

---

## TL;DR verdict

- **(a) Was the refactor completed?** **Yes — for its 2026-06-30 snapshot.** Every item the
  next-steps doc claims landed is present in the code, and all four local gates are green
  (reproduced this session): `cargo build` ✓, `cargo test` → **94 passed / 1 ignored / 0 failed** ✓,
  `cargo fmt --check` ✓, `cargo clippy --all-targets -D warnings` ✓. The only "not verified" item
  in the doc — the live DB-backed outage smoke — is still not run (needs a filled
  `DATABASE_PRIVATE_URL`/`DATABASE_URL` + local Ollama). That was a known, documented gap, not a
  defect.

- **(b) Merge cost onto `f043097`-era main?** **High — and semantic, not mechanical.** A
  `--no-commit` dry-run merge produces **8 conflicted files**. The important conflicts are
  *design collisions*, not textual: main independently did an overlapping refactor and **the stage
  roster drifted** underneath the branch.

- **(c) Recommendation:** **Leave parked; do NOT merge as-is** (merging as-is would regress main).
  If the stage-layer direction is still wanted, treat it as a **re-apply on top of current main**,
  not a merge. It does **not** gate the iOS App Store push either way.

---

## 1. Completion checklist (from `2026-06-30_rust-stage-layer-next-steps.md`)

Every "what landed" claim verified present in `2c6f08c`:

| Claim | Status | Evidence |
|---|---|---|
| New `rust/src/stage/` layout (mod, registry, runner, prompt + 6 stage files) | ✅ | files present |
| Old module paths kept alive via `lib.rs` re-exports | ✅ | `pub use stage::{headlines as headline, narratives, scrub, sigil, transfers as transfer, vibe}` |
| `StageSpec` + `StageHandler::spec()` metadata | ✅ | `stage/mod.rs` |
| Per-stage specs (roles / embedder / downstream) | ✅ | scrub=EmotionalNews+embed+downstream; headlines=EmotionalNews; transfers=EmotionalNews; narratives=EmotionalNews+embed+→vibe; vibe=EmotionalNews+→sigil; sigil=StatsLogic — matches doc exactly |
| Registry: `DEFAULT_STAGE_LIST`, `parse_enabled_stages`, ordered build, embedder detection | ✅ | `stage/registry.rs` (+5 unit tests) |
| Runner: elapsed logging, complete/retry/fail/defer bookkeeping, `preflight` | ✅ | `stage/runner.rs` |
| `work::defer` primitive (pending + last_error + available_at, **no** attempt increment) | ✅ | `work.rs:200` |
| Model preflight: `Inference::ping`, `Router::ping_role` | ✅ | `route.rs` (+2 unit tests, self-bound TCP listener) |
| `worker.rs` calls `runner::preflight` before claiming | ✅ | `worker.rs:119` |
| Live DB-backed outage smoke | ❌ (known gap) | needs real DB URL + Ollama; documented as not run |

Everything in the doc's "Next execution sequence" (§4 shutdown/cancellation, §5 lease heartbeat,
§6 expanded telemetry, §7 prompt contracts) and the "Carry list" is **explicitly future/roadmap
work**, not part of this refactor's completion. `prompt.rs` is intentionally just the
`PromptContract` metadata shape, as the doc states.

**Conclusion:** the structural refactor is complete and internally verified for the codebase as it
existed on 2026-06-30.

## 2. Build / test reproduction (this session, on the branch)

```
cargo build --all-targets        → OK (exit 0)
cargo test  --all-targets        → 94 passed; 1 ignored; 0 failed
cargo fmt   --all -- --check     → OK
cargo clippy --all-targets -D warnings → clean
```

- The **1 ignored** test is `embed::tests::paraphrase_beats_unrelated` — ignored because it
  downloads BGE-small (~130 MB) + runs CPU inference. Not a DB/Ollama gate; skip is expected.
- The `Router::ping_role` tests do **not** need a live Ollama: the "reachable" case binds a throwaway
  `TcpListener` on `127.0.0.1:0`; the "unreachable" case points at `127.0.0.1:9`. Self-contained.
- **No test in the suite requires a live DB or live Ollama.** The only real-world dependency is the
  manual worker smoke, which is not a `cargo test` and remains unrun.

## 3. Divergence & merge assessment (dry-run `git merge --no-commit --no-ff`, in a throwaway worktree)

**Auto-merged cleanly** (git rename detection worked): `scrub.rs → stage/scrub.rs`,
`stage.rs → stage/mod.rs`, `work.rs`, `worker.rs`, and the three new files
(`registry.rs`, `runner.rs`, `prompt.rs`).

**8 conflicted files:** `lib.rs`, `main.rs`, `route.rs`, `stage/headlines.rs`,
`stage/narratives.rs`, `stage/sigil.rs`, `stage/transfers.rs`, `stage/vibe.rs`.

The conflicts fall into three buckets, in increasing severity:

**(i) Mechanical** — `lib.rs`, `route.rs`: module list / re-export ordering and the `ping`
additions vs. main's route edits. Hand-resolvable in minutes.

**(ii) Stage-body collisions** — `narratives.rs`, `transfers.rs`, `vibe.rs`, `sigil.rs`:
these are the files main **rewrote across the 73 rust/ commits** (Phase 2 hot-path work, narratives
exclusions fold, oracle trigger + transfer pillar in sigil, etc.). The branch carries the
**2026-06-30 bodies** plus the structural move + `spec()`. Resolving each means keeping **main's
evolved body** and re-grafting the move + `spec()` on top — i.e. re-doing the refactor per file,
not accepting either side.

**(iii) Semantic roster drift — the blocker** — `main.rs` + `stage/headlines.rs`:

- **`headlines` was retired on main.** `rust/src/headline.rs` is **deleted** on main ("Headlines has
  been folded…"), so the merge shows `stage/headlines.rs` as **delete/modify (DU)**. The branch is
  reintroducing a retired stage.
- **Three new stages exist on main that the branch's registry knows nothing about:** `peak`
  (`rating::PeakHandler`), `momentum` (`momentum::MomentumHandler`), `oracle`
  (`oracle::OracleHandler`). Main's live roster is now **8 stages**:
  `scrub,peak,momentum,transfers,narratives,vibe,sigil,oracle`.
- **Main already did the overlapping refactor.** Main's `main.rs` has its own inline
  `parse_enabled_stages` + `build_handlers` for the current 8-stage roster. The branch replaces that
  with `stage::registry::build_handlers(&enabled)` — but `registry.rs` still encodes the **stale 6**
  (`scrub,headlines,transfers,narratives,vibe,sigil`). **Accepting the branch's side would drop
  peak/momentum/oracle and re-add headlines — a functional regression.**
- **The `spec()` metadata is also stale.** Main's embedder gate now loads the embedder for
  `scrub || narratives || vibe`, but the branch's `vibe` spec has `needs_embedder: false`. Merging
  the branch's `stages_need_embedder()` would **fail to load the embedder for a vibe-only run** — a
  behavior regression introduced by Phase 2's vibe changes landing after the snapshot.

## 4. What it would actually take to land the direction on current main

This is a **re-apply**, not a merge:

1. Rebuild `registry.rs` for the **current 8-stage roster** (drop `headlines`; add `peak`,
   `momentum`, `oracle`); keep main's registration order (oracle after sigil).
2. Move `rating.rs`(peak) / `momentum.rs` / `oracle.rs` handlers into `stage/` too, or accept a
   split layout — otherwise the "everything in `stage/`" goal is only ~5/8 satisfied.
3. For each of narratives/transfers/vibe/sigil: take **main's** current body, add the `spec()` +
   move; re-derive each `spec()` against **current** behavior (e.g. vibe `needs_embedder: true`).
4. Re-wire `main.rs` to delegate to the registry (deleting main's inline copy).
5. Keep `work::defer` + `ping`/`ping_role` + `runner::preflight` (these still apply cleanly and are
   the genuinely valuable, still-wanted parts).
6. Re-run all four gates + the still-outstanding live DB outage smoke.

Rough size: the mechanical move is cheap, but steps 1–3 are careful semantic work against ~73
commits of drift. Realistically a focused half-day-plus, dominated by re-deriving specs and
reconciling the four evolved stage bodies — **more re-implementation than merge.**

## 5. Recommendation

**Leave parked (do not merge as-is).** The refactor was genuinely finished and verified for its
moment, but `main` has both moved the stage roster and independently absorbed the same design intent
(env-driven registration), so the branch's central artifact — the registry — now encodes an outdated
world. Merging it would regress `main`; landing it correctly is a re-apply.

- If the **stage-layer/registry direction** is still wanted, schedule the §4 re-apply as fresh work
  off `f043097`. Salvage list: `work::defer`, `Router::ping_role`/`Inference::ping`,
  `runner::preflight` (highest-value, lowest-friction pieces).
- If not, the branch is safe to leave as a reference or eventually delete.
- **Either way this does not gate the iOS App Store push.**

*No merge, rebase, or delete was performed. `main` (f043097) and `wip/rust-stage-layer` (2c6f08c)
are unchanged except for this audit doc committed additively on the branch.*

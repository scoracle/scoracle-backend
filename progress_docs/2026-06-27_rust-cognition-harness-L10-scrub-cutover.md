# Rust Cognition Harness — L10: supervised daemon + scrub cutover (Cutover Step 1)

**Date:** 2026-06-27
**Plan:** vault `Plan - Rust Cognition Harness build.md` → "The Cutover Plan" (Step 1) + §7 ledger (L10)
**Status:** DONE — the `scoracle-cognition` daemon is systemd-supervised on archbox, and the news
**scrub** stage is cut over from Go-inline to the Rust queue handler (`NEWS_SCRUB_VIA_QUEUE=true`).
This is the FIRST stage of the Go LLM layer retired into Rust — the migration stall (0 stages cut
over) is broken.

## Goal

Execute Cutover Step 1: stand up a supervised Rust daemon (the operational prerequisite the L6 HELD
named) and cut over the one collision-free stage (scrub). Leave the cutover live + healthy, with an
instant one-flag rollback.

## Why scrub is the right (and only) Step-1 slice

The Rust worker drains sequentially and has **no GPU governor**; the Go derive worker has its own
(`OLLAMA_MAX_CONCURRENT` local-model gate). Two live workers draining stages they **share** would put two
uncoordinated callers on the single GTX 1070 Ti. **scrub is the one stage the Go Drainer has no
handler for** (it drains transfers/narratives/vibe/sigil only), so the Rust `ScrubHandler` can never
double-claim it. Every other stage waits for the full cutover (Step 3, Go derive OFF → Rust sole GPU
user). This is the hardware basis for "scrub first," not a convenience.

## Accomplishments

### Phase A — the supervised daemon (repo artifacts)

- **`scripts/systemd/scoracle-cognition.service`** — mirrors `scoracle-api.service`: `Restart=always`
  + `StartLimitIntervalSec=0` (a Postgres/Ollama restart can't wedge it), `EnvironmentFile=` `.env`
  then `.env.local` (the Rust crate reads `os::env` — it does NOT load `.env.local` itself, so these
  files are how the daemon gets `DATABASE_PRIVATE_URL` + `OLLAMA_*`, the same files the API reads),
  `Environment=COGNITION_STAGES=scrub` (the GPU-correctness constraint, documented inline),
  `KillSignal=SIGINT` (the worker's shutdown path catches ctrl_c/SIGINT, not systemd's default
  SIGTERM — so a restart finishes the in-flight drain cleanly instead of being SIGKILLed mid-item).
- **`scripts/systemd/scoracle-cognition.path`** + **`scoracle-cognition-restart.service`** — the
  rebuild-watcher, mirroring the API's. Watches the narrow **`rust/bin/`** dir (NOT the noisy
  `rust/target/debug/`, so an incremental build of an offline bin never bounces the live daemon).
- **`rust/bin/` deploy convention** (added `/bin` to `rust/.gitignore`): the deploy act is
  `cargo build --manifest-path rust/Cargo.toml && cp rust/target/debug/scoracle-cognition rust/bin/`
  — the `cp`'s close-write fires the `.path` restart, exactly like `go build -o bin/scoracle-api`.
- **`scripts/hosting/install.sh`** — renders the three cognition units (so a fresh checkout / archx220
  installs them) + a banner step. Rendered surgically into `~/.config/systemd/user` on archbox (did
  NOT re-run the full installer, to leave the live API units untouched).
- **Gate:** `cargo build` 0 warnings · `cargo clippy --all-targets -- -D warnings` clean ·
  `cargo test --lib` 35/35 (1 ignored real-model). Units parse (`systemctl … LoadState=loaded`). No
  Go source changed (the flag path + `ScrubHandler` shipped in L6).

### Phase B — canary (flag off): clean idle

Started the daemon with `NEWS_SCRUB_VIA_QUEUE` still false. Boot log, in order: connected to
Postgres · ollama reachable · loaded the BGE-small embedder (cached, ~245 ms, no HF fetch) ·
`registered stage handlers stages={"scrub"} handlers=1` · LISTEN `pipeline_work_ready` · **drains
nothing** (no scrub work enqueued; Go still owns scrub). 0 restarts. The `claim` is stage-filtered
(`WHERE stage='scrub'`), so the daemon never touches Go's stages; the only cross-stage touch is
`requeue_stale`, lease-age-bounded at 30 min (same threshold both sides) — the by-design shared-queue
recovery.

### Phase C1 — controlled proof (flag still off)

Manually enqueued 3 backlog articles as `scrub` work. The **supervised** daemon drained all 3 in
~3 s, writing real keep/drop verdicts (one article kept 3 players, **dropped** 2 teams — the
asymmetric gate's diviner judging). mig-103 then enqueued downstream work **only** for the 2 entities
with fresh (<72 h) vetted corpus — the 19-day-old articles' stale-only entities were correctly
suppressed by the trigger's freshness gate. Go derive drained **narratives=2, vibe=2, sigil=2 (0
fail)**. Full cascade, end-to-end, under systemd supervision. GPU healthy throughout.

### Phase C2 — the live flag flip

Set `NEWS_SCRUB_VIA_QUEUE=true` in archbox `.env.local`, restarted the API. The first maintenance
scrub tick fired the flag-on path: **`News scrub: enqueued to Rust scrub stage enqueued=15 failed=0
batch=15`** — Go enqueued 15 articles instead of scrubbing inline. The Rust daemon drained all 15 (0
warnings); their cascade enqueued **narratives=12, vibe=12, transfers=5**, which the Go derive worker
drained. **GPU stayed healthy** (mistral:7b 92% GPU-resident, no spill) with Rust scrub + Go derive
both active — the feared two-worker contention did not bite for this light, scrub-only slice (the
asymmetric gate auto-keeps ~50% on CPU; only the ambiguous band hits the model). Daemon: 0 restarts.

A temporary 2-min scrub interval was used to observe a real tick within the session, then **restored
to the 30-min default** (at 2 min, each tick's heavy cascade — 12 narratives is slow on one GPU —
outpaces derive; the default cadence clears it easily). The one-time backlog bump from the
observation window drains down at the normal derive pace; no new scrub until the next 30-min tick.

## Decisions carried

- **scrub-ONLY by deliberate constraint, not convenience** (the GPU-correctness finding). Don't add
  to `COGNITION_STAGES` without first retiring the matching Go drainer — two uncoordinated callers on
  one GPU is the failure mode. The clean end state stays the **full cutover** (Step 3).
- **`rust/bin/` + `cp` deploy** (not watching `target/debug/`) so offline-bin rebuilds never bounce
  the live daemon.
- **Per-machine `.env.local` flip** (archbox), like the L7 mistral cutover. The committed default
  stays `false` (safe). archx220 does not run the daemon (no mistral there yet — still open).
- **Rollback is genuinely one flag:** `NEWS_SCRUB_VIA_QUEUE=false` + restart API → Go resumes inline
  scrub; the daemon, finding nothing to claim, idles harmlessly (stopping it is optional tidiness).

## Landmines (carry)

- The Rust worker catches **SIGINT, not SIGTERM** → `KillSignal=SIGINT` in the unit so restarts
  finish the drain cleanly; a drain overrunning `TimeoutStopSec` is SIGKILLed + recovered by the
  30-min stale-lease (the backstop). A real SIGTERM handler in `worker.rs` is future polish.
- A **manual INSERT into `pipeline_work` does NOT fire `pipeline_work_ready`** (only mig-103 and the
  vibe→sigil enqueue do) → the daemon picks manual enqueues up via the 30-s safety-net, not instantly.
  Irrelevant to the live Go path (which goes through the trigger).
- The **maintenance scrub ticker fires on the interval, not once on startup** → after a flag flip +
  restart, the first enqueue is +interval away (used a temp 2-min interval to observe; restored to 30).
- **mig-103's freshness gate counts the ENTITY's <72 h vetted corpus, not just this article** →
  scrubbing old backlog correctly enqueues nothing for stale-only entities (the 19-day articles
  proved it; not a bug).
- The scrub backlog is **large (~30 k unscrubbed candidate-rich articles)** but the cutover only moves
  the maintenance ticker's 15/tick nibble — `NEWS_SCRUB_VIA_QUEUE` lives only in `maintenance.go`, NOT
  `cmd/pipeline` (the nightly bulk still scrubs inline; its cutover is Step 3 territory).
- `099_team_rosters.sql` still untracked (not ours). **F-046 still OPEN** (DB password in git history;
  a purge rewrites the cognition commits — coordinate before any force-push).

## Quick reference

```bash
# Deploy the daemon binary (fires the .path restart):
cargo build --manifest-path rust/Cargo.toml && \
  cp rust/target/debug/scoracle-cognition rust/bin/scoracle-cognition

# Daemon control / logs:
systemctl --user status scoracle-cognition
journalctl --user -u scoracle-cognition -f

# Cut scrub OVER to Rust (already done on archbox):  NEWS_SCRUB_VIA_QUEUE=true in .env.local + restart api
# ROLLBACK (one flag):  set NEWS_SCRUB_VIA_QUEUE=false in .env.local && systemctl --user restart scoracle-api
#   → Go resumes inline scrub; the daemon drains any residual scrub then idles.

# Fresh machine: scripts/hosting/install.sh renders all units; then (GPU box only)
#   systemctl --user enable --now scoracle-cognition.path scoracle-cognition.service
```

## File layout delta

```
scripts/systemd/
  scoracle-cognition.service          NEW — supervised daemon (Restart=always, COGNITION_STAGES=scrub)
  scoracle-cognition.path             NEW — rebuild-watcher on rust/bin/
  scoracle-cognition-restart.service  NEW — oneshot restart helper
scripts/hosting/install.sh            renders the three cognition units + banner step
rust/.gitignore                       + /bin  (the deploy target)
rust/bin/scoracle-cognition           (gitignored build artifact — the deployed daemon binary)
~/.config/systemd/user/scoracle-cognition.{service,path,-restart.service}   rendered (archbox; not in repo)
.env.local                            + NEWS_SCRUB_VIA_QUEUE=true            (archbox only; gitignored)
```

## Next — Cutover Step 2 (preview, not this session)

Port the missing stages offline, parity-gated, each before its cutover: **transfers** (author the t4
roundup/listicle clause + "never invent a fee/bid" — the L9 false-heat root, single-home in Rust),
**rating** (it's the `cmd/statcommentary` batch, not a queue stage), **narratives** (+ embed/cluster
dedup). Add a Rust **GPU governor** during this step. Then Step 3 = the full cutover
(`DERIVE_WORKER_ENABLED=false` → Rust sole GPU user; vibe/sigil fold in here). Still open: archx220
`ollama pull mistral:7b`.

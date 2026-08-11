# Handoff — gate `momentum-s11`, then decide the deploy

**You are on Scott's M4 Mac mini (16 GB), `192.168.1.77`.** This machine is the CHARACTER host:
it runs `ministral-3:14b` under ollama and serves Archbox over the LAN. Archbox
(`192.168.1.92`, `sheneveld@`) **is production** — it owns Postgres, runs the pipeline, and keeps
The Reader on `gemma3:4b`. Prod actions need Scott's named approval.

Repo `/Users/scotty/scoracle/scoracle-backend`, branch **`characters/peer-length-allowance`**,
4 commits ahead of `main`, tree clean, `cargo test --lib` = **242 passed**.

## The one job: gate `momentum-s11`

`s11` is the only change from this session that touches real code rather than prompt text — parser,
persisted score, a deleted clamp — and **it has never been run against a fixture**. Unit tests pass;
nothing has exercised it end to end.

```sh
cd /Users/scotty/scoracle/scoracle-backend/rust
export DATABASE_URL="postgres://unused/unused" OLLAMA_BASE_URL="http://127.0.0.1:11434"
COGNITION_ROUTE_MOMENTUM_LOGIC=ministral-3:14b ./target/debug/eval --task momentum --fixtures
```

No database needed — `run_fixtures()` is router-only (`eval.rs:400`); the fake `DATABASE_URL` just
satisfies `Config`. Takes ~3-5 min for 8 fixtures. **Run it backgrounded** — the default tool
timeout is 2 minutes and will kill it otherwise.

**What passing looks like.** The score assertions are GONE (the Analyst no longer emits a number),
so the remaining checks are prose-only: `prose_includes`, `prose_excludes`, `prose_min_words` (25),
`prose_max_words` (260). Specifically verify:

1. **Nothing is `unparseable`.** This is the real risk. The contract dropped to a single `READ:`
   line and `parse_momentum_reply` no longer requires `SCORE:`. If the model emits a bare paragraph
   with no `READ:` label, the parse fails closed. If that happens the fix is a prompt nudge, not a
   parser change — the label is what keeps the blurb clean.
2. **No `SCORE:` text leaks into the blurb.** The parser skips the line; confirm it isn't being
   captured as prose.
3. **Zero forbidden phrases.** `s10` took this from 10 → 0; it should stay 0:
   `grep -cE "the engine|the tape calls this|isn't a surge" <output>`

Baselines on the same fixtures: `s9` 37/42, `s10` 38/42 (both under the OLD contract, which had 2
score assertions `s11` removes — so the denominator drops, expect ~40).

## Traps that cost time in the last session

- **Archbox shares this GPU.** `OLLAMA_NUM_PARALLEL=1` means strict serialization: your eval calls
  queue ahead of production's. Check before taking it, and yield if Scott is debugging:
  `lsof -nP -iTCP:11434 | grep 192.168.1.92`
- **Don't prefix shell commands with `!` when Scott runs them in his own terminal.** In zsh a
  leading `!` is logical-NOT — it inverts the exit code. A successful `launchctl bootstrap` reported
  `exit=1` and looked like a failure for several minutes. The `!` prefix is Claude Code's convention,
  not zsh's.
- **Fixtures are regenerated offline**, no DB: `cargo run --example momentum_s6_fixtures`. The
  generators pull `*_SYSTEM_PROMPT` and `*_PROMPT_VERSION` live from source, so they always re-capture
  against current prompts. Same for `oracle_or4_`, `rating_s14_`, `vibe_v13_`, `narratives_n10_`.
  `transfer_t10_fixtures.rs` is pre-existing-broken (references the deleted `Harness.resolve`).
- **Don't replace-to-end-of-file when editing `tests.rs`.** Doing that silently destroyed three
  tests last session; the count dropping from 242 was the only signal.

## After s11 passes — the open list, in priority order

1. **Two Oracle regressions**, diagnosed and written up in
   `progress_docs/2026-07-26_mac-characters-regressions-and-workload.md`, fix directions included,
   **unapplied** because verifying needs the GPU. Both were caused by the longer allowance, and they
   share one mechanism worth understanding before touching the prompt: concrete nouns are finite, so
   a reading that doubles in length fills the surplus with imagery — which is entity-agnostic. The
   Oracle got MORE generic, not less. (R1: leaked `z-score` into a reading. R2: never named the
   entity at all, which its own rules call a non-reading.)
2. **Item 5 — the Oracle completion barrier.** Scott's design, agreed and scoped, not built.
   Replace the three `enqueue_sigil_for_*` calls (Analyst, Insider, Scout — the Journalist and
   Influencer reach the Oracle only *indirectly*, via `narratives → vibe → momentum → sigil`) with
   one `enqueue_oracle_if_pillars_settled()` called from all five pillar handlers. The barrier needs
   no migration: `work::complete()` DELETEs the row, so "no `pipeline_work` rows across
   `narratives/peak/vibe/momentum/transfers` for this entity" already means all five are settled.
   Two design points Scott accepted: count `status='failed'` as settled (a stuck pillar must not
   block that entity's readings forever), and enqueue downstream BEFORE checking the barrier or it
   can fire early on the chain.
3. **Calibrate the conviction ladder.** `momentum_conviction_from_score` thresholds are reasoned,
   NOT measured — chosen without sight of the live distribution. On Archbox:
   ```sql
   SELECT width_bucket(abs(momentum_score),0,100,10)*10 AS band, count(*)
     FROM public.latest_momentum_scores_per_entity GROUP BY 1 ORDER BY 1;
   ```
   If the mass sits under 20, most entities read ±1 and the 3/4/5 bands never fire.
4. **Deploy, when Scott says so** (see below).
5. **Gateway reservation** for `1c:f6:4c:72:20:72` → `192.168.1.77` — deferred by Scott, recorded in
   `HANDOFF-junctions.md`. Failure mode is total: a gateway reset moves the address and all six
   characters break at once with connection-refused.
6. **Reboot test.** The unattended chain (`autorestart 1` → FileVault OFF → LaunchDaemon `RunAtLoad`
   → `KeepAlive` → `sleep 0`) is fully configured and every link verified EXCEPT an actual reboot.
   Do it next time the box is power-cycling anyway.

## The deploy, when approved

Scott wants to run **Chelsea (`team:18:football`)** through the pipeline and look at the cards. That
cannot show anything new until this branch ships — Archbox still runs `n13/or5/v14/momentum-s7` with
the old 2-4 sentence ceilings.

```sh
# Mac
git checkout main && git merge --ff-only characters/peer-length-allowance && git push
# Archbox — atomic rename, never cp over a running binary (ETXTBSY), never pkill
git pull && cargo build --bin scoracle-cognition
cp target/debug/scoracle-cognition bin/.new && chmod 700 bin/.new && mv -f bin/.new bin/scoracle-cognition
```

The systemd **user** path unit auto-restarts. The version bumps sit inside `input_hash`, so every
character regenerates once per entity on the next sweep — expect a load spike at ~12 tok/s with
roughly doubled output lengths.

## What this session established, so it isn't re-litigated

- **The model is `ministral-3:14b` and that is settled.** It beat `mistral-nemo:12b` on the project's
  own frozen gate — JSON contracts 255/260 vs 253/260, plain-text 51/54 vs 49/54 — and decisively on
  the thing that mattered: given an identical "five or six sentences" instruction, ministral wrote
  **5.4** sentences and nemo **3.4**. Nemo passes checks while ignoring the length contract.
- **One model per machine. No routing to a second model, ever.** Scott is explicit: the multi-model
  system was wasteful. Archbox = gemma3:4b (Reader + graph, the high-volume layers). Mac = ministral
  (the six characters, the high-thought layer). Zero ollama model switches is the goal.
- **Ministral's earlier `0/42` on momentum was never capability** — it bolds plain-text labels
  (`**SCORE:** -1`) and the line parser rejected every one. Both plain-text seats now carry an
  explicit no-Markdown guard. Constrained JSON decoding masks the habit entirely.
- **The Analyst was miscast, and that is now fixed.** It never authored the momentum number; that
  number is the collision of the Scout rail and the emotional rails and already existed as the ±100
  `momentum_score`. Three prompt revisions failed to make ministral produce magnitude (it never left
  `{-1,0,1}` while nemo reached `-2` and `3`). `s11` computes it instead. **The Analyst voices the
  momentum; it does not decide it.**
- **`OLLAMA_TIMEOUT_SECONDS` was a stale 60 on Archbox and is now 600.** Resolved, not open.
- Ollama here is **version-pinned at 0.32.4** — the app no longer runs so it cannot self-update.
  Manual update = replace the app, then `sudo launchctl kickstart -k system/ai.ollama.serve`.

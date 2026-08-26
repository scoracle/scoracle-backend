# The resident model moves to granite4.2:3b — decision and switch record

**2026-08-25. Scott's decision, made on the fixture A/B below: the fleet's
resident model moves from `ministral-3:3b` to `granite4.2:3b` on both workers
(archbox + the Mac). The junction owns the character; the model is the engine,
and it is hot-swappable — this is the router's founding doctrine
("`OLLAMA_MODEL` is a route") exercised for real.**

## The decision

Scott, verbatim on the reasoning:

> The context win is MASSIVE, and the fact that it dominates in the expensive
> wins while losing in the cheap ones is revealing. […] My vision has always
> been the junction owning the character. The model is hot swappable. It's just
> the engine.

Strategic riders, also Scott's: Ministral's Microsoft deal and a departure on
their team; a preference for moving from European to American open weights.
Both models are Apache 2.0, so nothing legal changes — this is a sourcing
posture, not a license fix.

## The evidence — `bin/eval --fixtures`, all 9 tasks, temp 0, both Q4_K_M

Incumbent `ministral-3:3b` vs candidate `granite4.2:3b` with
`_CANDIDATE_THINK=false` on every role. Full outputs preserved in the
2026-08-25 articulator session scratchpad; re-runnable — the fixtures are the
reproducible gate.

| Task | ministral-3:3b | granite4.2:3b |
|---|---|---|
| oracle | 31/60 — **4/8 fixtures unparseable at temp 0** | **73/76** |
| editor | **60/60** | 56/60 |
| rating | **83/87** | 75/87 (7 of 12 misses are one ` · ` echo tic) |
| momentum | **95/95** | 93/95 |
| graph | **12/12** | 9/12 |
| transfer | 72/75 | **74/75** |
| vibe | 39/43 | **40/43** |
| narratives | 105/110 | 105/110 |
| investigator | **8/8** | 6/8 |
| total | 505/550 (91.8%) | 531/566 (93.8%) |

**The shape of the result is the argument.** Ministral's failures are the
expensive kind — parse failures (each a fail-closed retry burn; §3 of
SERVING.md is the 43-minute dead-letter window), `reading_max_peers` role
bleed, the ~2% foreign-script leak (`guards.rs::has_foreign_script`), transfer
stage misjudgment. Granite's failures are the cheap kind — a separator tic
copied from its own input, a missed tag, sentence-cap overruns. Formatting is
fixable at the input or the parser; judgment is not.

**Throughput, cache-defeated, same document both sides:** prefill wall-time is
a tie (Ministral 1,384 tokens @ 748 tok/s ≈ Granite 803 @ 438 — Granite's
tokenizer is ~42% denser); generation 44 vs 38 tok/s in Granite's favour. The
density is the headline: at `VOICE_NUM_CTX=4096`, the same window holds ~1.7×
more source material under Granite. The Editor's one-window constraint, the
Scout's `MAX_STAT_FACTS` truncation, the Journalist's news budget — every
ctx-budget decision in this codebase just got ~1.7× more room without a
config change.

## Thinking

Granite 4.2 has a native reasoning toggle; the fleet already has the per-role
knob (`COGNITION_ROUTE_<ROLE>_THINK`) from the qwen3-class encounter. Two facts
measured before the switch:

1. **Unset is unsafe.** With `think` unset on Granite, the scratchpad leaks
   into the card (a vibe hook shipped as its own word-counting deliberation).
   Every role MUST carry an explicit `_THINK` value — this is the sharp edge
   of the whole migration.
2. **Think-on is a quality knob with a latency price** (~2–3× wall clock, and
   it spends the same `num_predict` budget as the answer). Scott's call: with
   two workers holding the daily churn in the 4–5h target band, the headroom
   exists to spend on richer outputs.

Per-role think A/B (granite think-off vs think-on, same fixture gate, both
sides measured in one run at the 4096 envelope):

| Task | think-off | think-on | Verdict |
|---|---|---|---|
| transfer | 74/75 | **75/75** | **ON — the one win, and it is the judgment seat** |
| narratives | 105/110 | 105/110 | off (tie; latency not free) |
| oracle | 73/76 | 70/76 | off |
| investigator | 6/8 | 6/8 | off (tie) |
| graph | 10/12 | 8/12 | off |
| editor | 56/60 | 48/60 (2 unparseable) | off |
| rating | 77/87 | 67/87 | off |
| momentum | 90/95 | 65/88 (1 unparseable) | off |
| vibe | 40/43 | 25/43 | off |

**The mechanism is budget starvation, not bad reasoning.** The dominant
think-on failure is `prose_words_ge` (momentum ×8, vibe ×5): thinking spends
the same `num_predict` as the answer, so the prose seats — sized tight on
purpose — emit a stub. Transfer wins because its 700-token verdict budget
leaves the scratchpad room, and because `is_rumor` is pure judgment.
**Shipped: `COGNITION_ROUTE_TRANSFER_LOGIC_THINK=true`, all other roles
`false`.** The path to Scott's richer-outputs goal is per-seat: raise a seat's
`num_predict` constant (code, not env), re-run this gate, flip the role only
on a measured win — same discipline as the model swap itself.

**CORRECTED the same evening — thinking is OFF fleet-wide, transfer included.**
The chat-rail battery (thinking actually separating, +600 headroom funded)
disqualified it everywhere: transfer fell to **10/75** with think on. The
generate-rail 75/75 that earned transfer its flag was an artifact — that
endpoint never implemented thinking for granite, so the flag was inert and the
grammar did all the work. On a rail where thinking runs, granite deliberates
past every budget on every seat (the compression-doctrine failure in its pure
form). Both hosts' env now carry `_THINK=false` on all 11 roles, flipped
BEFORE the chat-rail binary deploys — the order matters, since the old binary
made the flag harmless and the new one makes it fatal. The chat rail itself
stays (equivalence verified: think-off matched the generate rail within
single-check jitter on all nine tasks), the `thinking` field stays on
`GenerateResult`, and `THINK_NUM_PREDICT_HEADROOM` stays — all three are the
correct plumbing for a FUTURE model whose deliberation scales with material
instead of contract mass. The doctrine that survives: on granite4.2:3b,
thinking is disqualified by nature, not by budget.

## The switch, mechanically

Per host (`.env.local` in the worker checkout; launchd sources it via
`run-worker.sh` on the Mac, systemd `EnvironmentFile` on archbox):

1. `ollama pull granite4.2:3b` (2.2GB, Q4_K_M).
2. `OLLAMA_MODEL=granite4.2:3b`; every explicit `COGNITION_ROUTE_<ROLE>=`
   line moves to `granite4.2:3b`.
3. Every role gains `COGNITION_ROUTE_<ROLE>_THINK=false` (or `true` where the
   think A/B table above says so).
4. Restart the worker (`launchctl kickstart -k gui/501/com.scoracle.cognition`
   on the Mac; `systemctl restart scoracle-cognition` on archbox).
5. Re-pin the resident: `curl localhost:11434/api/generate -d
   '{"model":"granite4.2:3b","keep_alive":-1}'`; release the old pin with
   `keep_alive: 0` for ministral once the worker is confirmed drawing.

Status: **BOTH hosts applied 2026-08-25.** The "archbox outage" was a DHCP
lease move (192.168.1.92 → .94; the box never went down — uptime 1d17h). Its
stack runs as USER-level systemd units (`systemctl --user`, not system) —
remember this before diagnosing "service not found". Archbox's worker was
restarted onto granite and its old resident unloaded (a 1070 Ti 8GB does not
hold two pinned 3Bs). The Mac worker additionally needs its `DATABASE_URL`
repointed to the new lease (or `.92` restored as an interface alias /
router-level static reservation — the durable fix for a warning SERVING.md
already carries). Queues were fully drained at switch time; the first granite
cards ship on the next trigger wave.

Rollback is the same edit in reverse; `ministral-3:3b` stays pulled on both
hosts indefinitely for exactly that reason.

## Dependency checks done before the switch

- **Vision:** `granite4.2:3b` has no vision capability; `ollama.rs` sends no
  `images` field anywhere — nothing routes images. Clear.
- **Judge separation:** `COGNITION_JUDGE_MODEL` stays `gemma3:4b` — it serves
  no seat, so the "judge serves no seat" rule survives the switch untouched.
- **Context:** both models exceed `VOICE_NUM_CTX=4096` by orders of magnitude.
- **Concurrency:** the Mac's `COGNITION_BACKEND_CONCURRENCY=6` was sized on
  ministral throughput; Granite's per-stream generation is faster, so the
  setting is kept and should be re-measured under real drain load, not
  pre-tuned.

## Guards and prompts: the model-agnosticism review

The audit finding is that the codebase already converged on the right law —
"rename the input rather than police the output" (applied seven times, per the
scout prompt history) — and the guards that remain are mechanical, not
model-tuned. What follows is the delta, not a rewrite:

1. **The one unapplied case of the one law: the Scout's datapoint render.**
   `format_datapoint_evidence` and the datapoints header feed the model
   `value · percentile · rating` notation, and `RATING_BODY_BANS = [" · "]`
   then hard-fails the echo. The 7B had this habit (8 of 9 rating reds,
   08-19); Granite has it too (7 of 8 fixtures). The fix is the same one that
   worked seven times: render the evidence with prose-safe separators
   (commas), so an echo is just grammar. The ban can then stay as a tripwire
   that never fires.
2. **Typography vs content is the right split — keep extending it.** `**` went
   from hard-fail to `strip_markdown_emphasis` at the parser (89 discarded
   felt reads over bolding, before). Granite's residual tics (occasional
   bullet lists, `#` headers) belong in the same strip family:
   normalize-at-parse, never reject-for-typography. Reject only content
   defects (bookkeeping citations, foreign script, role bleed).
3. **Model-specific residue that should stay** — it is protective regardless
   of engine: `has_foreign_script` (measured on ministral, harmless
   elsewhere), the U+2019 apostrophe handling in `contains_ci` (measured on
   ministral, correct forever), English-only guard on multilingual source
   material (analyst s7).
4. **Model-specific residue to retire when convenient:** prompt-history
   comments naming `ministral-3:3b` as the Editor's seat hardware
   (`editor/prompt.rs` header) — update on next touch, no version bump
   needed; SERVING.md §6's "currently `ministral-3:3b`" line — updated with
   this switch.
5. **The `_THINK` contract is now part of model-agnosticism.** Any future
   candidate model must be probed for (a) reasoning-mode default behaviour and
   (b) tokenizer density before an A/B is read — both moved the result here
   more than any prompt wording did.

## What this does NOT change

- The articulator's student trunk (Granite 4.0 H 1B) — separate decision,
  separate session.
- The judge (`gemma3:4b`).
- One provenance note for the articulator corpus: seat prose regenerated after
  this switch is Granite-authored. The five voices are the Articulator's
  sources, not its register, so mixed authorship is acceptable — but the
  post-drain re-extract should note the switch date if slice-level provenance
  ever matters.

## The 8192-window experiment (same day, Scott's ask): measured, and declined

Live-mode eval (real loaders, live corpus, temp 0), three busy entities × five
voice tasks, three conditions: 4096/think-off (prod), 8192/think-off,
8192/think-on. Outputs preserved in the session scratchpad (`ctx_ab/`).

**The window is not the binding constraint — the loader budgets are.**
Think-off outputs at 8192 vs 4096: rating and momentum **byte-identical**
(their evidence caps are constants that ignore the window), vibe identical
prose with a WORSE tail (the bigger `num_predict` let a duplication tic run),
oracle marginally different, and **narratives the one real win** — the
Journalist's `corpus_limit` follows the window and surfaced a third grounded
storyline the 4096 corpus cut had been hiding.

**Reasoning-on through the production `/api/generate` rail leaks on free-prose
seats.** Momentum: unparseable 3/3. Rating: the card came out as "We need to
produce four labelled lines in order:…". Vibe: character-counting scratchpad,
one empty card. Behind a `format_schema` the grammar holds it clean
(narratives terse-but-valid; transfer is the standing win). The rule this
measures: **think=true is only safe where constrained decoding owns the output
shape.** No change to the shipped think table.

**Decision: `VOICE_NUM_CTX` stays 4096.** A global flip is a no-op for most
seats, doubles KV per slot (archbox's 8GB card at 4 slots likely spills to
CPU — the Modelfile-window incident in miniature), and the one seat that
benefits has its own lever: `COGNITION_JOURNALIST_CORPUS_LIMIT` — though
raising it at 4096 must respect the window (the silent system-prompt eviction
hazard), so the Journalist's capacity move should ride the char-budget pass
below, gated by the fixture battery.

**Live defect found at TODAY'S prod settings (4096/think-off), fix before the
next drain wave:** on busy entities the vibe card carries granite's
self-review — "But wait—this doesn't quite fit the required format… Revised
VIBE:", "(Note: This stays within 6 sentences…)", and one card that restated
itself in full after "But the card must stay tight:". The fixture entities
never provoked it; live Lakers-sale-grade material does.

**Fixed (same day): `guards::truncate_self_review`, wired inside
`clean_served_prose`** so every served body passes it — journalist, insider,
influencer, oracle, scout, and the Analyst's READ (newly routed through the
shared scrub; her parser stripped Markdown per line but never took the shared
pipeline). Exact, case-sensitive markers measured from production output, cut
at the earliest, strip-not-reject; a body that OPENS with a marker truncates
to empty and fails honestly into the retry path. Tests carry the three
verbatim live defects. **Deliberately NOT added: a prompt rule against
self-review.** The material provoking the tic is the rules block itself, and
five prompt passes have proven a ban cannot beat its own material — the
salvage at the output is the guarantee; the prompt stays a guide (Scott's
pathway doctrine).

## The thinking unlock: the rail moves to `/api/chat` (same day, Scott's call)

Scott's read of the self-review defect, and it is the architecture: the
grading-in-the-answer-box tic and the thinking leak are the SAME phenomenon —
the model's deliberation has nowhere to go. His framing, kept because it names
the design: **the prompt+guards are a guide — the pathway. We give the
junctions the pathway, which gives them the platform to express themselves.**
The thinking channel is where the self-check belongs in that pathway.

The diagnosis that made it fixable: **Ollama's `/api/generate` does not
implement thinking separation for granite4.2 at all** — `think: true` returns
no `thinking` field and the scratchpad arrives in `response`. `/api/chat`
separates cleanly. So `ollama.rs` moved the rail to `/api/chat`:
`system`+`prompt` become the two-message array (same template application),
`GenerateResult` gains a `thinking` field (ledger/eval inspection ONLY — no
parser reads it, no card carries it), and the ledger-capture path in
`bin/eval` reads both stored body shapes. All 437 unit tests pass; the fixture
battery re-ran as the equivalence gate (results appended below).

What this unlocks, in order: (1) think-enabled roles stop leaking on
free-prose seats, (2) each seat's `_THINK` becomes a real quality knob gated
only by its `num_predict` headroom, (3) the prompt can GUIDE the channel —
"check the contract in your reasoning; the reply is the card, written once" —
which is the pathway doctrine applied to deliberation itself.

**Deploy note:** the production workers run pre-cutover binaries until
rebuilt (the `.path` units restart them on rebuild). The endpoint change
rides the next worker rebuild on each host.

## Landmine found and defused during the switch: the Modelfile window

Granite4.2's Ollama Modelfile ships `PARAMETER num_ctx 131072`, and a model's
own parameter **outranks the server default**. Ministral shipped no such
parameter, so the codebase's convention that an omitted `num_ctx` means "the
server's 4096" was true for every model this fleet had ever run — until this
one. First omitting call (the eval fixtures path, `num_ctx: 0`) loaded a
**25GB KV instance onto the 16GB Mac**, 55/45 CPU-split, and the ollama
service's `OLLAMA_KEEP_ALIVE=-1` pinned it there. Generation slowed to a
crawl; on archbox the same load would blow straight past the 135W power cap's
working set.

Fix (committed with this doc): `ollama.rs::build_request` now ALWAYS sends
`num_ctx`, resolving `<= 0` to the 4096 envelope. Production junctions were
never exposed — every one already passes an explicit window — but the eval
harness was, and the general law is now enforced at the one choke point:
**never inherit a model's own default; state the window on every call** (the
same contract as per-role `_THINK`). All 437 unit tests pass.

Measurement note: the head-to-head A/B above ran granite through the omitting
path, i.e. at a 131k window. Temp-0 output for a prompt that fits the window
is identical regardless of window size, so the scores stand; only wall-clock
was affected, which is why the throughput axis was measured separately with
explicit `num_ctx=4096`.

## Fixture debt found in passing

- `investigate_entity/` has a fixture directory but no registered task.
- oracle `near-empty-quiet-wire` is frozen at `or12`; the task is at `or13` —
  re-capture + re-annotate (the harness's own fixture-rot warning flagged it).

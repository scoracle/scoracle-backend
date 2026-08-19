# 2026-08-18 (night) — three gates: Scout seat, Mac contender, triage Reader

Context: capacity is the open watch-item (player products accrue ~800/day over the drain).
Plan explored: a 1B pre-Editor "Reader", more seats to the 1070, MLX for the Mac.
All gates ran in the 23:00 rest window. Logs in `logs/model-eval/`.

## 1. Scout → ministral-3:3b — REJECTED

`rating-AB-defiant9b-vs-ministral3b-20260818-2300.log`. Incumbent defiant-fable:9b (Mac),
candidate the 3b (archbox, via tunnel). Property checks **tied 94/95 vs 94/95** — the prose
decided it, same as the Oracle A/B of 08-15:

- **Fabrication**: 78th percentile → "a 78% success rate"; "per-36" → "concede 36 minutes
  of possession"; invented targets ("force turnovers to 1.8 or lower per-90", "70%+ catch
  rate") — none on the decision cards.
- **Inversion**: "Force a high-percentage pull-up shot from the 95th percentile … to limit
  his scoring" — advises forcing the opponent INTO his elite skill.
- **Guard violations**: builds an exploit from a 58th-pct average mark directly after
  correctly stating "no clean exploit exists"; "rim protection" in a soccer profile.

The Mac lane stays **4 seats**: Journalist, Influencer, Scout, Oracle. Scout briefs feed
the Analyst and Oracle, so fabricated numbers would propagate as card facts. A retry goes
through scope/contract narrowing first (the Analyst precedent).

⚠ All 8 rating fixtures warned `fixture-rot` (frozen s18, task now s19) — re-capture due.

## 2. granite4.1:8b as the Mac model — PARKED (not rejected)

`vibe-AB-…-granite8b-20260818-2322.log`, `oracle-AB-…-granite8b-20260818-2322.log`.
Checks: vibe 52/53 vs 49/53; oracle 146/148 vs 138/148. The hypothesis (token efficiency
up, emotional reads down) confirmed:

- Outputs ~20–25% shorter at equal compliance elsewhere; 26 calls in 9 min incl. cold load.
- Vibe: news-summary register, not the felt read; leaked heat-70 as "70% confidence";
  blew the 12-word hook cap twice.
- Oracle: pastes REAL card internals ("mood of 80/100", "form trend (0.1 over five
  samples)"), said "sentiment" in 5/8 readings, named 4 peer seats in one reading.

Distinction that matters: granite is **faithful-but-indiscreet** (real numbers, wrong
place); the 3b is **confabulatory** (fake numbers). The former is the prompt-fixable class,
and the frozen prompts are qwen-tuned — granite never got an adapted prompt. Revisit only
if the Mac lane still binds after the Reader lands. Model left pulled on the Mac.

MLX rule set the same night (Scott): Ollama is the default; leave it only for proven
performance gains. defiant-fable exists only as GGUF locally, so MLX would need HF
safetensors + convert + re-gate (different 4-bit quant), on `mlx_lm.server`, D-T53 method.

## 3. granite4:1b-h triage Reader — VIABLE, v1 numbers in hand

Speed (archbox, `num_gpu:0` — pure CPU, zero VRAM): ~500 calls/hr single-stream at the
title+RSS+2k-chars shape (~7s clean; ~825/hr at title+RSS only) vs ~2,000 triage
decisions/night. Constrained `{page_kind, hypothesis_entity_present}` JSON held 100/100 calls.

Quality (50 Editor-labeled articles, sport-correct hypothesis join — first sample had a
cross-sport join bug, discard v2 numbers): with the **editor-mirror rule** (kill on
non-reporting page_kind OR hypothesis-team absent, derived in code per T2):

- **false-kill 1/25 — and the 1 is the Editor's own false positive** (a hospital press
  release the Editor marked success; the 1B correctly called it other/absent)
- **kill recall 10/25 (40%)**; strict AND-rule: 0% / 28%
- page_kind agreement 34/50 vs the Editor envelope

Prompt learning that mattered: say explicitly "a short transfer/match brief IS an
article" — without it the 1B calls briefs `listing_or_schedule` (v2's false kills).
Recall headroom: the missed irrelevants are odds/preview/how-to-watch pages whose bodies
are sentences; a targeted betting/broadcast clause is the next iteration.

Net v1 prize: ~40% of the ~40% irrelevant class ≈ **16% of Editor model calls
(~30–40 min of 1070 GPU/day)**, growing with recall. Build requirements if picked up:
triage kills must write terminal `editor_reads` rows (team-coverage ≥80% watchdog and
D-T21 cap accounting both key on that table); harness + labeled sample preserved in the
session scratchpad (`granite-agreement2.py`).

## 3b. Reader prompt iteration (00:10–00:33, same 50-article sample) — FROZEN at R3

- **v4** (dense "purpose-based" taxonomy, 5 new clauses): COLLAPSE — false-kill 80%.
  The 1B lost the presence question entirely under the longer instruction (called
  "Real Madrid refuse loan bids", hypo Real Madrid, absent). Lesson: the 1B's
  instruction budget is ~one clause per iteration; dense taxonomies destabilize both
  answers, not just the one being tuned.
- **v5** (v3 + ONE odds/summary clause): editor-mirror rule hit 64% recall but 20%
  false-kill — the clause re-poisoned the `listing_or_schedule` label (4 transfer
  briefs). The 1B's listing label swings with prompt wording; **score_table and
  hypothesis-absence are stable across all prompt versions**.
- **FROZEN: the R3 tiered rule — kill on `score_table` OR hypothesis-team absent;
  ignore the 1B's listing/roundup labels.** Identical results on v3 AND v5 prompts:
  40% recall, 1/25 nominal false kill (the Editor's own hospital-article false
  positive), i.e. ~0 real coverage loss, prompt-robust.
- **Bonus finding — regex before the model**: 12/25 of the irrelevant class have
  self-identifying titles (`Prediction and Odds`, `How to watch`, `Live Score`,
  `Betting Tips`, `Team Stats`, `Game Summary`, `Live Stream`, `ai prediction`,
  `NBA Odds`). A deterministic title pre-filter kills 48% for zero model calls and
  near-zero risk; union with the 1B-R3 layer = **60% recall, ~0 false kills ≈ ~24% of
  Editor model calls saved (~45-60 min of 1070 GPU/day)**. Design implication: the
  Reader is regex-first, 1B-second — code before model, per house doctrine.
- Remaining misses are judgment-heavy (speculation columns, media-industry news) where
  the Editor's own verdict needed entity_roles subtleties — not reachable at 1B, fine.

## 4. 08-19 midday (daemon manually paused 12:28–12:46): two more gates

### 4a. Sport-relevance Reader (Scott's make-or-break) — FAILED, Reader DROPPED

One question ("is this page about {sport}?", FOOTBALL/NFL/NBA each glossed), 30 success +
40 irrelevant. Result: **the 1B answered true on all 70** — including MLB articles fetched
under FOOTBALL queries (Guardians under Clermont, Brewers under Salernitana) and American
football under FOOTBALL (Saints tryouts under Southampton). 0% false kills, 0% kill
recall. Same all-true collapse as triage v1 — boolean yes-bias plus the football/football
trap. Additionally the prize was small: eyeballing the sample, only ~15-20% of the
irrelevant class is wrong-sport at all. Per Scott's stated criterion ("if not, we'll drop
this"), **the Reader layer is dropped.** The shape+presence R3 config (§3b: 40% recall,
~0 false kills) remains on the shelf as the only Reader design that measured viable.

Ops finding from the failed first runs: a generate for a non-resident model on the shared
archbox ollama TIMES OUT (curl rc 28) while the drain owns the card — any future second
model on archbox needs its own ollama instance/port, and F-035's planned
MAX_LOADED_MODELS=1 makes that mandatory.

### 4b. granite4.1:3b vs ministral-3:3b on the card — MINISTRAL HOLDS, decisively

Daemon stopped (clean numbers). Checks: editor **60/60 vs 55/60** (editor fixtures are
current-spec — cleanest signal), momentum 90/91 vs 77/91, transfer 72/75 vs 68/75.
Granite's failure signature mirrors the 8B on the Mac — a family trait, not a size fluke:

- **Momentum**: "steady band" (banned) leaked in 8/10 fixtures; digits pasted into
  no-digit prose repeatedly.
- **Editor**: entity under-extraction (missed Clement + Rangers on the Dragojevic
  fixture); envelope errors that flip the code-derived relevance BOTH directions
  (Saka injury read → derived irrelevant; Paris fixture page → derived relevant).
- **Transfer**: named the TEAM as subject instead of the player (Miami Heat for Austin
  Reaves, Detroit Pistons for Jalen Duren); missed a true-positive rumor.

The "better instruction following" hypothesis did not survive contact at either tier on
frozen prompts. The directing-layer conclusion stands regardless: had the discretion
guards existed in production, granite's momentum violation rate (8/10 vs ministral's
~1/10) would be a dashboard number, and every future contender gets measured for free.

⚠ More fixture-rot: all 10 momentum fixtures frozen at s15 (task is s16); all 8 transfer
fixtures at t6 (task is t11 — five versions stale). A/Bs stay fair (identical frozen
prompts both sides) but the baselines need re-capture.

## 5. 08-19 13:08 — ministral-3:8b vs defiant-fable:9b re-gate (ollama, fixed parsers)

The fair fight the 08-12 A/B (oMLX era) never had. Checks were near-parity — vibe
**53/53 (8B) vs 52/53 (9B)**, oracle 143/148 vs 146/148, rating 94/95 vs 95/95 — and the
8B is vastly improved from its oMLX-era record, confirming the engine confound was real.
**The prose reading still keeps the seat with the 9B**, on two findings:

1. **Vibe score calibration overshoots hard** — `clearly-negative`: 8B scored **18** vs
   the 9B's 42 (target ≤40, drawn so an honest read sits at the boundary);
   `warm-memory-cold-coverage`: 8B scored **14** vs 42 (target ≤45). The 8B turns a
   quiet 4-game slump into an obituary, numerically. vibe_scores feed the momentum
   computation, so this is numeric corruption of the downstream rails — the worst
   defect class, and unguardable (no string scan catches "18 should have been 40").
2. **Invention beyond the corpus** — "Trade chatter hums at **zero heat**" where the
   card says heat FOUR (the 9B voiced it correctly); "he's gone… already out of time…
   narrative already written" for a card describing shrinking minutes, not a departure.
   Direct violation of the Influencer guard (emotion must trace to the corpus).
   Notably the 8B's *register* is genuinely punchy — it writes like an influencer —
   but it dramatizes past the facts it was handed.

Oracle side: discretion leaks in the granite class but milder — peers=3 in one reading,
"the omen is" ×2, an asterisk in plain text, sentence-cap misses, score inflation
(92 vs 82 on ascendant-aligned). Mostly guardable strings; the vibe calibration is not.

**Consequence: qwen won outright, on fair evidence.** The MLX-for-Mac path therefore
runs through defiant-fable's original weights: DavidAU publishes a source-files
collection for his merges (Apache-2.0), but this exact merge's full-precision weights
are unconfirmed there. If present → mlx_lm.convert + full re-gate (different quant =
different model). If absent → MLX is closed for this seat, and Mac throughput levers
reduce to prompt/output diets and duty-cycle scheduling.

## 6. 08-19 afternoon — the fun-run, and the eval→guard migration SHIPPED

### 6a. 12B Thinking HERETIC (fun-run, 13:51–15:06)
Vibe 51/53 — genuinely good: scored 42 on `clearly-negative` (identical to the 9B),
decent register, no visible think-block leak into prose. Oracle 125/130 with several
candidate calls timing out outright; rating collapsed to 35/48 — missing the three
required section labels, `**` everywhere, and (caveat) that leg ran 14:11–15:06 against
RESUMED production with model-swap thrash, so its numbers are contaminated. Verdict:
never a candidate, but two real datapoints — (1) an inline-thinking model can hold
register and calibration while failing latency budgets catastrophically (thinking cost
lands as 600s timeouts); (2) independent confirmation the vibe fixture targets are fair.
Gate-runner lesson: the script only re-pauses the daemon if IT stopped it — a gate that
spans an odd→even boundary needs its own pause regardless of starting state.

### 6b. Implementation landed (all tests green, 398 passed)
- **`guards.rs`** — one home for the served-prose guide rails: PRODUCT_NAME_BANS,
  MOMENTUM_BANNED_PHRASES, new ORACLE_READING_BANS + RATING_BODY_BANS + VIBE_BODY_BANS,
  hook contract, peer-name counter, digit scan, fold/contains_ci. `eval_tasks` imports
  from it — the gate measures exactly what production enforces.
- **Architecture ruling** (from the test failures, kept deliberately): raw `parse_*`
  fns stay SHAPE-ONLY; guards live in the `Parser` seam (production). The eval gate
  parses the raw fns, so a violating reply still shows its prose and scores red —
  production rejects, the gate diagnoses. `parse_rating_body` added for the same
  reason; rating eval no longer goes through `RatingParser`.
- **Guards wired**: Analyst (banned phrases, product names, digits — beside the
  foreign-script precedent), Oracle (reading bans, peer roll-call >1, product names,
  foreign script), Influencer (hook contract, body Markdown, product names, foreign
  script — `VibeParser` can fail closed for the first time), Scout (` · `, `**`,
  product names — first rejection path), Journalist (product names on served fields).
  Every rejection logs `tracing::warn!` with the guard name → violation-rate telemetry.
- **Vibe v19→v20, the CALIBRATION pass**: scenario anchors in the SCORE block; stale
  v14 doc headers fixed; changelog records the 18-vs-42 evidence.
- **Fixture refreezes**: vibe 5 @ v20, rating 8 @ s19, momentum 10 @ s16 — via their
  generators (real-builder rendering; expects in-generator per the ep7 lesson).
  **Transfer only 2/10 refroze** (t11): the other 8 incl. all `live-*` are production
  CAPTURES, not generated — the t6→t11 re-capture is its own project (needs DB +
  re-annotation).
- Post-refreeze baseline sweep (9B on vibe v20 + rating s19; 3b on momentum s16) run
  same window — the v20 anchors' first reading on the incumbent.

## Follow-ups

- [x] Re-capture stale fixtures — DONE 08-19 for rating (s19), momentum (s16), vibe
      (v20); transfer 2/10 done, remaining 8 are production captures → own project
- [ ] Transfer fixture re-capture t6→t11 (8 captured fixtures, needs DB + re-annotation)
- [ ] WATCH after deploy: the 3b's "steady band" leak (2/10 momentum fixtures at temp-0,
      i.e. near-modal on steady items) now trips the production guard → retry at temp 0.3.
      Expected to clear on re-roll (the foreign-script precedent), but watch the
      `momentum_banned_phrase` warn rate + dead letters the first days. If it creeps:
      momentum prompt nudge, not guard removal.
- [ ] Post-refreeze baselines recorded (15:13 sweep): vibe v20 52/53 on the 9B — anchors
      are calibration-neutral for the incumbent (same lone target-red, scores unchanged);
      rating s19 94/95 ("play physical" contextual red); momentum s16 89/91 on the 3b.
      These are the comparison numbers for the Phase 2 lean-8B gate.
- [x] Reader decision — DROPPED 08-19 per Scott's sport-relevance criterion (§4a);
      shape+presence R3 design shelved as the only viable variant
- [ ] Promote discretion rules from eval checks to production guards with retry
      (banned terms, peer-seat names, digits in no-digit prose) — model-blind
      directing layer; also yields per-model violation-rate telemetry
- [ ] F-035 still open (archbox systemd `OLLAMA_NUM_PARALLEL` drop-in, needs sudo)
- [ ] Granite family: both tiers lost on frozen prompts (Mac 8B §2, card 3B §4b);
      revisit only with a prompt-adaptation pass AND guards in place first

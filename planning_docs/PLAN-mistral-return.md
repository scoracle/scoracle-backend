# PLAN — the Mistral return: direction work → lean-8B gate → MLX

*(Opened 2026-08-19. Doctrine: `DOCTRINE-directing.md`. Evidence base:
`progress_docs/2026-08-18_seat-and-reader-gates.md`. Goal: quality holds, and the Mac
lane reaches the 354–443/hr band (4–5h daily clear) via ministral-3:8b lean quant
(~1.19× bytes) × MLX batching (~1.5–2×) ≈ 1.8–2.4× lane throughput.)*

**Backlog context:** the lane does ~180–190/hr on defiant-fable:9b and the queue grows
~800/day. Phase 2 alone (~1.19×, no engine risk) slows the bleed; Phase 3 clears it.

## Phase 1 — Direction work (model-blind; benefits the 9B regardless)

### 1a. Vibe score calibration
An honest correction to the working theory: the score scale is NOT missing —
`influencer/prompt.rs:52-62` already carries bands ("reserve under 15 / over 90 for
seismic moments", "quiet cycle → 40-65"). The 8B scored 14 and 18 *through* those
bands. So the work is strengthening direction, and Phase 2 tests whether the 8B can
follow it — if it still miscalibrates, that is a model limit, honestly found.

- Sharpen the SCORE block with scenario anchors (a benching + low-heat chatter in a
  quiet week ≈ 35–45; protests + winless month ≤ 25; a routine win in a flat week ≈
  45–55). Keep it short — the block is read by every model forever.
- Close the asymmetry with the Journalist: n20 has a grammar-enforced score range +
  eval band axes; v19 has a soft clamp and no format schema
  (`eval_tasks.rs:626-636`). Add the same hard range enforcement for vibe where the
  reply format permits.
- Mechanics of the bump (v19 → v20): `VIBE_PROMPT_VERSION` (`prompt.rs:91`) is FIRST
  in the debounce pre-image (`mod.rs:257-260`), so a bump forces exactly one regen per
  entity as its pipeline wakes — cheap and already designed for. Re-freeze the 5 vibe
  fixtures via the generator, and per the ep7 lesson: **author expect keys in the
  generator, not by hand-editing JSON** (see fixture-expect-keys memory; verify the
  denominator).
- Fix the stale doc header while in there (`prompt.rs:9` still says v14).

### 1b. Discretion guards (eval checks → production)
Design rulings:
- **One list, one home.** `PRODUCT_NAME_BANS`, `MOMENTUM_BANNED_PHRASES`,
  `count_named_peers`, and `fold_for_match` already exist in `eval_tasks.rs`
  (L1698-1761) but are gate-side only. Move them to a shared module (`util.rs` or a
  new `guards.rs`) and have BOTH eval and production parsers import them —
  eval_tasks.rs:1707-1718 already records the "the ban is global, so it belongs in one
  place" ruling; this completes it.
- **Retry = the work queue, not an in-process re-roll.** The `has_foreign_script`
  precedent (`analyst/mod.rs:385-394`) rejects in `Parser::parse` → `Err` →
  `retry_backoff` (30s/2m/10m/30m, dead-letter at max). Guards follow it. Accepted
  cost: a violation burns a backoff cycle, not a fast re-roll — fine at the expected
  ~1/10 rates for resident models; a high-violation model getting slow is the point.
- Insertion points (from the 08-19 seam survey):
  - Influencer: `parse_vibe_reply` before the Ok at `influencer/mod.rs:575`; note
    `VibeParser` currently NEVER fails closed (doc L617-621) — this change makes it
    able to. Scan hook + body.
  - Oracle: `parse_crown_reply` after reading-normalize (`oracle/mod.rs:953-956`) —
    peer-name scan + banned vocab + "the omen is".
  - Scout: `RatingParser` (`scout/mod.rs:1252-1258`) currently always `Ok(Some(..))` —
    add rejection path; product-name + banned-phrase scan.
  - Journalist: per-narrative scan in `NarrativesParser` (`journalist/mod.rs:251-271`).
- Digit guards: reuse the `prose_no_digits` semantics (momentum READ); decide per
  field — momentum blurb yes, oracle reading yes-with-allowlist? (spelled-out numbers
  are house style; ASCII digits are the leak signature).
- Telemetry: log each guard rejection with junction + model + guard name — the
  violation-rate dashboard falls out of grep/pipeline_runs later.

## Phase 2 — Seat gate on the deploy build
- Candidate: `hf.co/bartowski/mistralai_Ministral-3-8B-Instruct-2512-GGUF:IQ4_XS`
  (5.55GB, pulled 08-19). Byte edge vs the 9B: **~1.19×** (the 25% hope died on the
  scale — 8.9B params + vision tower ride along; a vision-stripped or IQ3 build could
  buy more at quality risk — only after IQ4_XS is judged).
- Gate ALL FOUR voices: vibe, oracle, rating, **narratives** (no challenger has faced
  narratives yet). Frozen fixtures re-captured post-Phase-1 (v20). Prose reading
  decides; calibration is the named axis to read hardest.
- Pass → optional immediate Ollama cutover (~1.19× now) while Phase 3 gates.

## Phase 3 — MLX server eval (only after Phase 2 passes)
- Server: `mlx_lm.server`; model: `mlx-community/Ministral-3-8B-Instruct-2512-4bit`
  (exists, Apache-2.0 — no conversion work). NOTE: MLX 4-bit ≠ IQ4_XS — the MLX build
  is a THIRD distinct model and needs its own fixture pass before cutover.
- D-T53 method in rest windows: 30/30 completion at sustained req/h vs Ollama;
  aggregate tok/s at 2/3/4/6 concurrent; RSS growth over a long run (oMLX grew ~3GB).
- **The named risk: grammar/structured-output integrity on the tekken tokenizer** —
  the oMLX xgrammar corruption was tekken-specific. Test constrained decode
  (editor-style schema + journalist card_score schema) explicitly before trusting it.
- Protocol gaps + mitigations: no per-request num_ctx → fix server-side; no think
  flag → moot, ministral doesn't think. Memory envelope rules apply (one model,
  launch agent, desktop app quit).

## Phase 4 — Cutover
- D-T57 gate baseline on the exact deploy config; flip `.env.local` routes; watchdog
  eyes on the first duty cycles; keep the 9B pulled for instant rollback
  (`launchctl kickstart` + route revert).

## Decision gates
1. Phase 2 prose reading fails on calibration after 1a → model limit; stay on 9B,
   keep the guards, close the plan honestly.
2. Phase 3 grammar test fails on tekken → ship Phase 2 on Ollama (~1.19×), MLX closed.
3. Both pass → full cutover, expected 1.8–2.4×.

# 2026-09-20 — The evidence contract ships to the model

One investigation arc, three migrations (264–266), six harness corrections, one
release. The Scout hallucination investigation that started with "why does Scout
invent blocks and turnover causes" closed by fixing the evidence the model was
never given. Everything below is committed and, as of this session's deploy,
LIVE.

## The root finding

Every stored NBA/NFL rating breakdown carried `measure == label`
(`Rim Protection` → `Rim Protection`), so the adapter suppressed the duplicate and
the model received only the category name. Migration 252 had fixed football's
derived formulas; the NBA and NFL branches never got the same treatment. The
model wasn't inventing — it was interpreting an arbitrary curated label as a
statistical fact. Read-only probes against production confirmed the incidence
before anything was written.

## The migrations (in deploy order)

1. **264 — source measurement identity.** `rating_measurements` /
   `rating_measurements_team` name the underlying quantity or formula for every
   NBA/NFL label (Rim Protection → blocks, Playmaking → assists, Ball Security →
   turnovers, On-Court Impact → plus-minus; NFL formula labels get explicit
   formula text; team branches too). Plain labels keep themselves. All NBA/NFL
   bundles rebuilt in-transaction with a parity proof: every breakdown member
   except `measure` identical, else rollback. FOOTBALL branches byte-identical
   to 252. Deployed incidence confirmed read-only first.
2. **265 — withhold z when the comparison spread is undefined.** The 253
   standardization coalesced division-by-zero-spread to z=0, so a lone eligible
   entity rendered `quality z +0.00` — which the legend reads as "exactly
   average". Percentiles were already withheld for these populations; z now is
   too. Parity: a withheld z still contributes its former sign·0 to the composite
   (SUM over an all-degenerate population would otherwise collapse to NULL and
   erase the composite — caught during local testing). DuckDB port mirrored.
3. **266 — per-measure cohort size.** Each ranked observation carries `cohort`,
   the COUNT(*) of the ranked same-measure population, threaded through
   pop/z/scored into the stored breakdown. A percentile without its population
   invited magnitude guessing; now the model can size it. Purely additive —
   parity proves every pre-existing field identical.

## The harness changes (prompt contract s55 → s59)

- **s55**: the Scout brief states the sport-id mapping (NBA basketball, NFL
  American football, FOOTBALL association football) after the Arsenal card wrote
  "pressure on the offensive line" — the assignment named the sport only by raw
  id. Ablation: naming the sport killed the cross-sport idiom 12/12.
- **s56**: thin-sample omissions moved into code — `model_prompt_profile` drops
  Discipline before the top-2 selection instead of handing it to the model with
  an order to ignore it; the "Omit participation totals and Discipline" sentence
  retired. The withholding is ledgered (`thin_sample_omitted_in_selection`).
- **s57**: "Also measured but not selected for this assignment: …" — the
  exclusion ledger reaches the model as one label line, so selection is visible
  rather than declared. Empty exclusions render nothing.
- **s58**: ranked evidence reads "percentile 95.0 (elite) of 4470 eligible
  profiles" when a population exists; no population, no number.
- **s59**: the header renders the curated `sports.display_name`
  ("Football (Soccer)") from the database — no Rust sport dictionary.
- **Guards, no new bans**: the thin-sample stability guard became claim-shaped
  (cross-time stability or "no change"/"relative standing" violate; a
  within-snapshot description of standing is free prose), direction verbs tied
  to named measures are rejected on thin samples (no per-measure trend exists),
  and the one-appearance boundary joined the no-comparison guards it previously
  escaped.

## Verification trail

- In-migration parity proofs: 264 (everything except `measure` identical), 265
  (everything except `z`, and every z change must be 0 → NULL on an eligible
  unranked observation), 266 (everything except the additive `cohort` key).
- `sql/tests/253/264/265/266` all pass on disposable databases; Rust 509 → 514
  library tests, all 57 database tests (single-threaded, against an isolated
  migrated database), Go analytics equivalence green on 16 real cases.
- Frozen rating fixtures: user prompts unchanged by s56–s59 (their synthetic
  profiles carry no thin-sample boundary, exclusions or cohort); pins moved per
  the drift check.
- Real-path per-sport test on production-slice data (player + team for each
  sport) through `build_rating_request` → `scout::create`: no pipeline errors;
  one guard rejection fired correctly; the rich-cohort NBA cards were the
  cleanest, confirming the evidence-contract hypothesis.

## Where this leaves the product

The harness now tells the model what was measured, what was selected, how big
the comparison was, and which sport it is writing about — and sheds
prohibitions instead of adding them. Still open: per-measure source coverage
(missing vs provider-suppressed zero, bound to the provider contract), and the
thin-sample inversion readings (3B model quality, not evidence).

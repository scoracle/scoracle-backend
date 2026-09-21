# Harness alignment audit — September 20

Read-only audit of the Scout harness against the studio's design goal: give the model the studio —
complete, well-labeled, source-truthful evidence — while letting it paint freely (selection owns
what is claimed, prose owns what it means, no growing ban lists). No code changed in this audit.
Each finding cites observed behavior from the disposable seeded database and the frozen prompts
from this week's per-sport runs.

## Aligned: the harness now supplies the studio truthfully

1. **Source measurement identity at the producer** (migration 264): every NBA/NFL label carries its
   real quantity or formula; parity proven in-database. The model no longer interprets a category
   name that the SQL selected arbitrarily. This is the core "give the model the studio" fix.
2. **Conditional magnitude interpretation** (s54 legend): magnitude interpretation is asked only
   when a quality z is supplied; percentile standing stays available on its own. The legend states
   polarity once and forbids double inversion — evidence description, not prohibition.
3. **Selected-evidence framing** (s54): "Selected evidence: measures omitted from this assignment
   are not thereby zero or unavailable in the source." — selection is declared rather than hidden.
4. **Sport disambiguation** (s55): the voice names the sports factually. The ablation showed the
   ambiguity, not model banality, produced cross-sport vocabulary ("offensive line" for Arsenal).
   Naming a sport is identity context, not a statistical cause — consistent with the handoff rule
   that identity cannot establish causes.
5. **Claim selection is code-owned**: `model_prompt_profile` (comparison movements + held anchors,
   or two facts for thin samples) and `ordered_facts` (≤14, both ends sampled) are deterministic;
   `RatingExclusions` records what selection dropped, and the ledger persists the reasons
   (`application/scout.rs:517-545`). Routing (`compute_notability`) is outside writing context.
6. **Abstention is real**: the schema accepts JSON null, the parser treats it as a completed pass,
   the marker carries called-provenance, and the Analyst respects the latest marker.
7. **Hashing**: `input_components` carries label, measure, value, pct, sign and z; prompt_version
   gates prompt-contract changes (now s55). Provenance reflects what produced the card.

## Drift: where the harness still fights itself or the model

### 1. Instruction patches where selection should be (thin-sample boundary)

`build_stat_prompt`'s thin-sample paragraph orders the model to "Omit participation totals and
Discipline." But selection already owns omission: participation details are not even rendered for
thin samples (the sample line says "participation details omitted"), and `Discipline` is dropped
from the prompt only by instruction, not by `model_prompt_profile` — a thin-sample player whose
top-2 percentile facts include Discipline is handed the datapoint and then told to ignore it.
This is selection leaking into prose instructions: the code should drop it (one filter), and the
sentence should retire. Same pattern as the handoff's user direction: selection is shared with the
model, but the model should never be handed evidence it is simultaneously ordered to ignore.

### 2. Constraint duplication across three channels

The same boundary is now enforced in the prompt instructions, the body ban list
(`RATING_BODY_BANS`: "reduced minutes", "reduced playing time", "mid-season", …) and the
thin-sample guard (stability words). Three channels for one contract means every future evidence
change must be edited in three places, and lexical bans accumulate rather than retire. The ban
list's genuinely evidence-format guards (" · ", "exact labels and bands", "printed measurements")
are fine; the claim-shaped entries duplicate instructions and should be retired as the evidence
contracts make them redundant. Nothing new should be added to that list.

### 3. Lexical guards miss the real failure they were built for

Observed live: the thin-sample stability guard **rejected** a card for "consistent defensive
discipline" (benign, band-supported) while a **passing** card said the defense showed "a
measurable weakening in ... preventing scoring" against percentile-100.0 elite Goals Against. The
stability ban is word-shaped; the inversion is claim-shaped. The band-contradiction guard exists
and is evidence-anchored, but it only fires when the model names a band word in the same clause as
the label. A polarity-anchored check — a claim of weakening/strengthening a negatively oriented
measure whose band is elite/poor — would replace several lexical rules with the evidence itself.

### 4. The studio still has holes the model fills by guessing

These are the handoff's unresolved items, still open, each observed in this week's runs:

- **Degenerate-population z is fabricated neutral.** The standardization coalesces a division by
  an undefined spread to 0 (`schema.sql:1178` and four team/cohort paths). In the slice test, a
  lone eligible player rendered `quality z +0.00` — and the legend says "0 is average," so the
  model can read a fabricated neutral distance. Percentile is correctly withheld for the same
  population; z should be withheld the same way (explicit degeneracy state, not a new formula).
- **Per-measure comparison population is invisible.** The model sees "percentile 96.0" with no
  cohort size; the cohort memory block carries peer counts only for composite deltas. A supplied
  population size would let magnitude phrasing stay proportional without any rule.
- **Missingness reasons are absent per measure.** Missing vs inapplicable vs provider-suppressed
  zero vs not-selected remain indistinguishable at the datapoint level; 264 carried identity but
  not coverage state.
- **Exclusions never reach the model.** The ledger records exactly why labels were dropped
  (budget, off-facet, degenerate zero, display tier), but the prompt carries only the generic
  selected-evidence line. One rendered line ("also measured, not selected: …") would complete the
  studio view — the model currently cannot distinguish selection from absence, which is precisely
  the expansion seam the pressure audit documented.

### 5. Sport naming is voice-embedded, not data-driven

The s55 mapping lives in the character brief. `sports.display_name` already curates the text
("Football (Soccer)"). Moving it into the user prompt header keeps voice text stable when a sport
is added — noted as follow-up; requires a version bump and fixture refresh, so it waits.

## Alignment check of this week's changes

| Change | Studio (evidence truth) | Freedom (no over-reach) |
|---|---|---|
| Migration 264 identity | Supplies true quantity/formula, parity-proven | No interpretation dictated; parity means no numbers changed |
| s54 legend corrections | States what percentile/z can support | Magnitude questions conditional — asks less, not more |
| s55 sport mapping | Names the sport factually | No vocabulary prohibition; 13 words |
| `scout_freeze` example | Real preparation path, frozen prompts | Diagnostic only; `-sport-label` ablation mutates a copy |

## Prioritized next work (evidence/harness levers, no new bans)

1. **Withhold z when the spread is undefined** (SQL: carry a degeneracy state; renderer already
   omits missing z). Parity-safe like 264: rebuild inside one transaction with before/after proof
   that all non-degenerate z are unchanged.
2. **Move thin-sample omissions into selection**: drop Discipline in `model_prompt_profile` for
   thin samples; retire the "Omit participation totals and Discipline" sentence (the sample is
   already omitted upstream).
3. **Render the exclusion ledger as one prompt line** ("also measured, not selected: …" by reason)
   so selection is visible rather than declared.
4. **Anchor the inversion gap in evidence**: extend the band guard to polarity language
   ("weakening/strengthening" clauses against a supplied band), and retire the matched lexical
   stability words once the anchored check covers them.
5. **Carry per-measure cohort size and coverage state** in the bundle (new migration, additive
   fields, same parity-proof pattern as 264).
6. Data-driven sport display name in the user prompt (after a deliberate version bump).

Nothing in this audit recommends a model comparison, a larger ban list, or prompt prohibitions;
every item either supplies evidence the model is currently missing or moves a constraint from
prose into code where the design places it.

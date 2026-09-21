# Scout pressure audit

## Question

Why does Scout expand a sufficient supported claim (for example, high relative standing on a measure) into unsupported mechanics, roles or inflated judgments? This is distinct from whether an empty assignment can abstain.

## Method

One model, Ornith 1.5 9B, and one frozen rich Scout case. Current s52 prompt, shared form, claim selection and card-or-null schema remain the baseline. All calls are first-generation diagnostics, not production parser/rewrite runs. Raw requests and responses: `logs/studio-pressure-20260920/`.

The first four calls isolate two factors: remove the two-sentence qualitative/graph instruction; remove the four raw measurement values while retaining names, percentiles and quality bands; remove both; change neither. A fifth removes only the instruction to judge difference size using z-score magnitude. A sixth replaces only the Scout character brief with a short statement of its subject. No model comparison, production route change, database write or production prompt edit was made.

## Confirmed context defects

1. **The legend assigns a task for which the fixture has no input.** It says to use z-score magnitude to distinguish a material difference from a tiny one with an extreme percentile. No quality z is present in this fixture. In the baseline response, the turnover judgment becomes “a real weakness, not a rounding error.” The single omission changes that certainty to uncertainty about the size of the problem, but leaves other unsupported claims. This is evidence of sensitivity, not proof of a unique cause.
2. **A category name is not necessarily a measurement definition.** The fixture supplies `rim_protection`, not a definition of the quantity. Repository SQL derives NBA Rim Protection from `blk`, but returns the category label itself as `measure` (migration 252, NBA branch). The Rust renderer omits duplicate measure/label text. Underlying measurement semantics therefore can disappear before model interpretation. This needs source-contract work, not a prohibition on the word “blocks.” No deployed database state was checked.
3. **The frozen rich case is synthetic, not a capture of the production adapter.** Its defensive-rebound and screen-assist lines are not the current NBA player rating_measurements list. It remains useful for testing interpretation, but cannot establish which live source fields are fabricated. Earlier reports calling a blocks interpretation fabricated must be read as “unsupported by this fixture,” not “wrong for the live underlying measure.”
4. **The character brief repeatedly requests synthesis.** It asks to connect the measurements, read them together, describe their relationships, and write what they mean together. Meanwhile, shared form already asks for a coherent supported claim. This is a plausible expansion pressure; omitting the graph sentence alone does not fix it.

## Observed results

| Change | Outcome |
|---|---|
| None | Exact prior rich response reproduced: invented turnover causes, a defensive sequence, and demand for a full offensive profile; 1951 body characters |
| Remove qualitative/graph instruction | Still invents a defensive role; treats strong screen assists as an oddity that does not fit that role; 1726 characters |
| Remove raw values | Still invents passing touch from screen assists, asserts neither defensive measure is a sample-size fluke, and upgrades 88th-percentile rebounding from strong to elite in the headline; 1423 characters |
| Both omissions | Invents team first-line defensive responsibility, reason for court time, and small/manageable turnover impact; 1295 characters |
| Replace Scout brief with subject only | Still invents passing lanes, deterrence, passing ability and a causal connection between defensive activity and turnovers; repeats material-difference judgments despite missing z-scores. Shortening the character brief is not sufficient. |
| Remove unavailable z-magnitude task | No longer asserts the turnover gap is materially large; still invents ball movement/rolling and on-every-possession implications, and inconsistently claims peer turnover comparison is absent despite its supplied percentile |

The four factorial responses all ended normally with parseable JSON, but all exceed the 1200-character body limit. None is a grounded quality pass. Omitting those two factors is not a demonstrated fix.

## What the evidence rules out

The first-generation errors predate retries. The captured request has only the explicit system instruction and fixture user evidence: no injected prior Scout prose or few-shot exemplars. The model/server accepts JSON null in a separate compatibility check. These failures do not require a forced non-null grammar, raw numeric values, or the graph sentence. Claim selection in shared form is retained throughout.

## Interpretation

The repeated failure is semantic expansion: relative standing becomes a playing identity; other supplied measures are then interpreted through that identity. A 96th-percentile measure is already a sufficient positive claim. Abstention addresses no-claim cases, but does not address extending a valid claim beyond its evidence. The evidence supports a relationship between the measured contributions, not every sporting mechanism or causal story associated with their category names.

Do not declare this audit a solved root cause. The source contract loses some measurement definitions, the prompt requests unavailable z-score interpretation, and the brief repeats broad synthesis demands. Each is concrete and testable; none of these experiments alone establishes a universal cure.

# Product alignment + prompt fit pass

**Scope:** Product narrative alignment, model-neutral language cleanup, local-model prompt tightening, and one live-data shadow test against Chelsea (`team:18:FOOTBALL`).

## Product alignment

- News is the package product: headlines, transfers, and narratives are grouped under the news surface instead of treating headlines as a separate product.
- Momentum remains its own endpoint/product because it tracks rating trajectory and vibe trajectory over time.
- Sigil now consumes `momentum_score` as a pillar input so the crown can reflect trajectory, not just accumulated stat profile + latest news/vibe.
- Public/internal docs were updated toward generic local-model language so the system is model-interchangeable as hardware changes.

## Model-neutral cleanup

- Removed model-specific product/docs labels in favor of local-model/model-neutral wording.
- Renamed transfer AI summary storage/wire language toward `model_summary` / `summary`.
- Added migration `115_model_neutral_ai_labels.sql`.

## Prompt fit changes

The cognition prompts were tightened for a small local model: shorter instructions, schema-first output rules, fewer redundant role/personality tokens, and explicit false-signal guards where the model had been over-inferencing.

- Headlines: `h1 -> h2`, `num_predict 2000 -> 1200`.
- Transfers: `t4 -> t5`, `num_predict 1200 -> 900`.
- Narratives: `n3 -> n4`, `num_predict 4000 -> 3000`.
- Vibe: `v7 -> v8`, `num_predict 1200 -> 512`.
- Rating: `s6 -> s7`, `num_predict 2000 -> 1200`.
- Sigil: `s5 -> s6`, `num_predict 1000 -> 512`, now includes `momentum_score` in input components/hash.
- Resolve prompt was simplified.

## Chelsea shadow test

Ran current Rust shadow harnesses against `mistral:7b` for Chelsea (`team:18:FOOTBALL`) without writing live product rows.

| stage | live baseline | current shadow | result |
| --- | --- | --- | --- |
| narratives | `n3`, 6 rows, avg impact `32.0` | `n4`, 5 rows, avg impact `36.6` | tighter narrative grouping from 25-item corpus |
| vibe | `v7`, score `65` | `v8`, score `65` | concise felt read, same score |
| rating | `s6`, notability `90`, peak `Elite possession play` | `s7`, notability `90`, peak `Elite Passing and Possession Control` | same score, cleaner label |
| sigil | `s5`, score `64` latest live during test | `s6`, score `54`, `momentum_score=26` | trajectory grounded explicitly in the crown |

Prompt sizes from the persisted shadow rows:

- narratives: `5405` chars, `num_predict=3000`
- vibe: `2349` chars, `num_predict=512`
- rating: `1247` chars, `num_predict=1200`
- sigil: `3387` chars, `num_predict=512`

## Verification

- `cargo test --lib` passed after the prompt/code changes.
- Chelsea shadow harnesses passed:
  - `NARRATIVES_PARITY_VET=1 cargo run --bin narratives_parity -- team:18:FOOTBALL`
  - `cargo run --bin parity -- team:18:FOOTBALL`
  - `RATING_PARITY_VET=1 cargo run --bin rating_parity -- team:18:FOOTBALL`
  - `cargo run --bin sigil_parity -- team:18:FOOTBALL`

## Notes

- The compact personality/voice lines appear useful as style constraints, but they should stay operational and tiny. The Chelsea run did not show evidence that those voice constraints overloaded Mistral-7B.
- Sigil input components now carry `momentum_score`; in Chelsea's test row that value was `26`, alongside `latest_composite=17.2` and `latest_sentiment=65`.
- Local-model temp-0 output is still not treated as a deterministic parity axis; persisted prompt/request/hash/version fields remain the regression signal.

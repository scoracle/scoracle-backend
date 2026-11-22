# Five-article development test

Journalist, Insider and Influencer now receive **at most five articles per request**, ranked by the decision model's character-specific score: `subject`, `movement` and `emotion`, respectively. Ties retain incoming order; missing or invalid scores rank after valid scores, including zero. Production delivery and fixture replay use the same selector. Lower-ranked delivery obligations remain pending and are not acknowledged as used. Existing identity, source and duplicate checks remain; these can leave fewer than five articles.

The JSON order is **job → context_guide → meta + fresh**. There is no trailing `tone` or `context_guide.tone` for these three characters. Voice is stated in the task. Their instructions were shortened, particularly Journalist's long worked-example block. The output contracts and factual validation rules remain. Citation keys and report indices refer to the selected, ranked reports; parsers and schemas use that same selection.

The earlier **29,705 tokens was a total across twelve requests**, not one request. These are the **complete native input tokens for each new request**, including system instructions and the chat template:

| Character / target | Articles before → now | Previous input | New input |
| --- | ---: | ---: | ---: |
| Journalist / Chelsea | 24 → 5 | 9,929 | **2,459** |
| Journalist / Demichelis | 4 → 4 | 4,247 | **2,102** |
| Journalist / Beal | 3 → 3 | 4,956 | **2,811** |
| Insider / Neal | 7 → 5 | 3,507 | **2,162** |
| Insider / Roma | 9 → 5 | 4,372 | **2,456** |
| Influencer / De Zerbi | 8 → 5 | 3,434 | **2,369** |
| Influencer / Jones | 2 → 2 | 1,625 | **1,412** |

All seven inputs are below 4,096 tokens. The reduction combines fewer articles, score ordering, shorter instructions and removal of tone framing; this experiment does not isolate their individual effects. The prior Chelsea figure was tokenizer-only because the full packet did not fit its earlier admission budget.

## Results

Seven local `Qwen3.5-4B-Q4_K_M` generations on the existing endpoint: temperature 0, thinking disabled, concurrency 1, no retries. All finished normally, with 88–158 generated tokens each. Six passed mechanical checks; Neal was rejected by the unchanged source/name validator. No generation was run for Scout, Analyst or Oracle in this test.

Beal's jersey story is now separate from the injury reporting. Jones's own disappointment and the former player's criticism now keep their correct actors, but the output still incorrectly calls the two negative reactions mixed. Material errors remain: Chelsea advice acquires an unsupported speaker and intent; Roma invents a coaching role for Pellegrini; Neal's destination is paraphrased as a competitor. Influencer exceeds the requested one or two sentences. These are development observations, not held-out accuracy results.

- [Complete outputs and manual source review](outputs.md)
- [Complete system and JSON inputs](prompt-preview.md)
- [Selected article IDs, exact scores and excluded IDs](selection.json)
- [Native per-request token measurements](input-tokens.json)
- [Machine-readable review and receipt links](review.json)

IDs, links, publication dates, wording and qualifications within each selected article are unchanged. Original full candidate parts remain in fixtures so ranking can be replayed and audited. Stored IDs and source links are preserved even though they consume input tokens.

## Verification and limits

The five-article limit is a temporary tuning constant in `src/tools/source.rs`. Shared tests cover score ordering, stable ties, zero versus missing/invalid scores, the three character mappings, tone removal and request/schema selection. All 29 affected quality fixtures were regenerated through production assembly. Other characters retain their previous input bytes, instructions and versions; only the shared assembler's optional-tone call signature changed for them.

The input target is not a new hard 4,096-token context-window setting. Trials retain the existing 10,240-token admission context and unchanged output reservations (Journalist up to 2,100, Insider 1,400, Influencer 600). Reserved input plus output for Journalist can exceed 4,096 even though its measured input is well below it. Actual input plus generated output was below 3,000 in these seven calls. Five unusually long articles or URLs could still exceed an input budget; native admission continues to hold oversized requests rather than clipping evidence.

Final verification is recorded in `verification.json` and `tests.log`. No database access, publication, deployment, model download or historical-context work occurred. The unrelated production-recovery planning document was preserved.

Prompt versions: Journalist `n127-top-five`, Insider `insider-source-v28-top-five`, Influencer `vibe-frame-v40-top-five`.

Reproduction: build the `character_package` and `character_outputs` examples offline; run `build_cases.py`, then `measure_inputs.py`. `run_outputs.py` runs only these three characters and requires a fresh output directory. `review_outputs.py` verifies the generation token counts against native preparation and renders the saved review. Preserve existing receipts before creating a new trial.

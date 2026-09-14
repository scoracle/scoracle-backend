# Compact memories, unchanged Scout voice

Four local first-pass calls, same granite4.2:3b route, seeds 17/43, 4,096-token
window, temperature 0.6, 700 output reservation and shared form as the first run.
The renderer now uses compact semantic fields, and statistics explicitly name
`played_for` and unknown coverage. No production writes or surface retries.

Full prompt: 1,520 tokens, down 466 (23.5%) from the initial 1,986. Removing the
performance group yields 1,320 tokens. This is a context-size result, not an
editorial success or a full RSS-request capacity test.

| Condition | Seed | Body characters | Parser |
|---|---:|---:|---|
| Full | 17 | 1,258 | Fail: hook/body surface |
| Baseline removed | 17 | 1,050 | Pass |
| Full | 43 | 1,423 | Fail: body surface |
| Baseline removed | 43 | 1,494 | Fail: body surface |

**All four rejected.** Transfer dates are invented from observation dates; source
coverage becomes actual appearance totals or an adaptation/fitness explanation.
Seed 43 full also confuses reporting intervals with competition progress. The
ablated seed 43 invents a one-appearance Villa career. No automatic prose repair
was used and no keyword ban was added.

`requests.json`, `responses.jsonl` and `summary.json` preserve exact inputs, raw
responses and production-parser review. The package is a reviewed fixture with an
official-web overlay, not a capture from the newly wired runtime source adapter.

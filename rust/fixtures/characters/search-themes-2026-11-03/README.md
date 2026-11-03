# Search context and recurring themes — November 3, 2026

The next Journalist, Insider and Influencer prompts are implemented as **experimental candidates, not deployed**. The first comparisons do not establish a general quality improvement. Regular review production remains paused on both hosts; classifier tuning on Archbox was not changed.

## Corpus completed before testing

The early November 2 edition finished its required assignments at **November 2, 11:29:23 PM Detroit**. The snapshot taken November 3 at 11:41:50 UTC confirms:

| Character | Evidence assignments | Saved reviews | Qualified reviews |
| --- | ---: | ---: | ---: |
| Insider | 193 | 192 | 176 |
| Influencer | 410 | 372 | 328 |
| Total | 603 | 564 | 504 |

Of 603 assignments with evidence, 504 produced qualified candidates, 27 abstained and 72 ended blocked/failed. These outcomes include source and completion checks; they are not 72 model-content rejections. Sixty of 564 saved reviews were unqualified. Empty assignments are separately recorded as no-new-evidence and excluded from this denominator. Counts cover the overnight Insider/Influencer run, not the earlier Journalist corpus. See [the aggregate receipt](overnight-corpus.json).

At the user's request, the Mac LaunchAgent `com.scoracle.cognition` was disabled and booted out, and only `scoracle-writer.service` was stopped on Archbox. Model services, classifier service/configuration, collection, API, queues and retries were left unchanged. The Mac model remained available at `127.0.0.1:8081`. Resuming the Mac writer requires `launchctl enable gui/501/com.scoracle.cognition` and bootstrapping its existing LaunchAgent plist; the Archbox writer requires starting its existing service. Neither was resumed by this experiment.

## Prompt change

Versions are Journalist `n136-search-themes`, Insider `insider-source-v37-search-themes`, and Influencer `vibe-frame-v49-search-themes`.

Each plugin's `fresh.rs` retains its existing interpretation rules and adds:

> Headlines found through a Google search for the target may include unrelated stories, repeated coverage, speculation and clickbait. Search inclusion does not establish relevance or confirm a claim. Focus on clearly relevant stories; do not turn a question or implication into a fact. Look for recurring themes supported by distinct reports. Repeated copies are not independent support. Keep unrelated stories separate; do not force a narrative from thin evidence.

Influencer additionally says:

> Distinguish coverage tone from evidence of how people actually feel. Sensational wording alone does not establish strong public emotion.

The conditional search wording avoids claiming that every legacy excerpt or future evidence provider came from Google. It supplies search-result interpretation without adding guessed provenance. The three character guides remain independently tunable; no shared assembler or classifier files were changed for this experiment. Tasks, system rules, target data, source wording/order, ten-article cap, schemas, output limits and parsers are unchanged.

## Controlled comparison

Eighteen deliberately selected development cases cover six per character, teams/players/people, three sports and one to ten headlines. Journalist cases come from the preceding retained production baseline; Insider and Influencer cases come from the completed overnight corpus. Cases include saved/qualified reviews, unqualified reviews and no-product outcomes. Some targets were examined in earlier development trials; this is not a held-out sample.

All eighteen baseline user payloads rebuild **byte for byte** from the saved requests through the production assembler. Influencer uses retained structured components directly. Journalist and Insider components were reconstructed from the saved writing payload; internal discovery/ranking metadata absent from that projection was not invented. Selected order and complete model input were verified unchanged. This tests writing on frozen evidence, not replay of live upstream selection or publication.

An isolated source copy of deployed commit `73a9882f62f4` keeps the concurrent classifier/shared-module edits out of the comparison. Both variants use the same provider preparation, native schemas and evaluation parsers. Baseline/candidate order alternates by case. All inference ran on the Mac: Qwen3.5-4B-Q4_K_M, temperature 0, thinking disabled, concurrency 1, no retries, no database access or publication. Requested context is 10,000; native server context receipts report 10,240. Input counts include system instructions, user JSON and chat framing; decoding schemas are separate.

| Measurement, 18 paired cases | Baseline | Combined candidate |
| --- | ---: | ---: |
| Strict validation passed | 9/18 | 11/18 |
| Complete native input tokens | 26,365 | 27,997 |
| Per-request native input range | 1,033–2,153 | 1,116–2,236 |
| Output tokens | 2,706 | 3,734 |

Input increases **1,632 tokens, or 6.19%**: 83 per Journalist/Insider request and 106 per Influencer request. Every candidate input stays below 4,096 tokens. Output volume increases substantially, partly because one candidate repeats its recap. Strict evaluation includes content checks that observation-mode production may allow; these results are not publication counts or factual accuracy rates.

Manual overall judgments are **3 better, 4 worse, 11 with no clear change**, with limitations still present in improved cases. One history-only case abstains more appropriately; two Influencer readings become more precise. Regressions include repetitive paragraphs, an invented agent-hiring prerequisite, invented affiliation/fan sentiment, unsupported interpretation of a quarterback headline, and a damaged hook. Actor reversal, a sporting reunion misread as employment movement, an invented contract-expiry date and a negative-emotion packet scored as mixed remain. A newly accepted Insider answer is factually worse. See [case-by-case review](review.json) and [native token/check receipts](results.json).

## Six-case diagnostic follow-up

Twelve additional calls separate search/noise guidance from theme guidance on cases 2, 5, 8, 9, 13 and 15. Search-only also includes Influencer's coverage-tone distinction. These are new prompt variants, not retries or additional passes in production. Each variant passes strict checks on 4/6 cases; the same subset passes 2/6 at baseline and 4/6 with the combined candidate.

Neither isolated component reproduces the combined variant's long repetition loop in this run, but theme-only introduces source/actor errors and search-only damages a hook. Both variants still misread the sporting reunion and give the negative-emotion packet a mixed score. All three additions variants abstain on the history-only case, so that benefit cannot be isolated to search provenance. The themes-only agent-hiring paragraph avoids the combined variant's contradiction and invented prerequisite, but still misses a separate story. These six selected cases do not establish a winning component. See [ablation results](ablation-results.json).

Total: **48 completed model calls**, zero transport/admission errors in this trial, and all strict failures retained. Further tuning should isolate actor relationships, covering each story once, hook completion, and score meaning. Do not promote the combined prompt based on the 9/18 to 11/18 validation change.

## Verification and private replay

The isolated candidate passed **429 tests; 74 ignored**, using `cargo test --offline --lib --examples --bin eval`. Two existing loopback HTTP tests initially failed because of sandbox socket permissions; the unchanged suite passed with loopback access. Both logs are retained. Formatting and scoped diff checks pass. This verifies the deployed-source baseline plus this prompt change, not integration with concurrently changing classifier code.

Thirty-one current quality fixtures were regenerated; twenty-six unrelated requests remained byte-identical. The assertions in [recapture.py](recapture.py) verify that only the first content-guide entry changes: target, evidence, field definitions, task, system, schema and settings stay identical. Historical experiment fixtures remain unchanged. [Verification details](verification.json) include source hashes.

Raw saved production cases, exact requests, raw generated replies, strict failure receipts and the readable paired review remain in the private Mac directory:

`/Users/scotty/scoracle-worker/experiments/search-themes-20261103/`

The private `review.md` presents the supplied headlines and both outputs for each case. Production archive originals remain on Archbox and the Mac; no raw production payloads or outputs were added to this repository. [run_pairs.py](run_pairs.py) records the bounded Mac-only paired workflow and requires a private experiment directory. Its output directory must be new. Request/response hashes in the aggregate results identify private receipts without copying their contents here.

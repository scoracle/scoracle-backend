# Native reader and combined intake experiment — October 19

`dom_smoothie` is fast and small enough to remain a candidate for the backend. Its default extraction is **not qualified as article evidence**: pages without their main story can return recommendations or legal footers instead. The decision spectrum and character parsers did not consistently stop those failures.

The later [strict-policy rejection exploration](#strict-policy-rejection-exploration) below blocks the three known bad examples while retaining the six reviewed articles, with one known substantive article missed elsewhere in the saved batch. It is an offline candidate, not a deployment.

The user proposed one intake job that reads, decides relevance and character routes, retains relevant material, and passes that shared body to the plugins. That job boundary is reasonable. This experiment checked its components without changing production or the local v8 admission implementation.

## What ran

The native Rust probe linked the current backend and pinned `dom_smoothie` 0.18.2 in a temporary Cargo project. The repository's application dependencies and lockfile were unchanged. Native `BudgetedFetcher` handled the real Google RSS URLs, public-address checks, redirects, response limits, four-way concurrency and domain spacing. Only a temporary PostgreSQL database was used.

The sample was the complete frozen 100-URL batch from the October 18 browser test, with its 122 canonical subject cases. HTTP responses were fetched on the M4 on October 19. It is real corpus material, but not a fresh discovery sweep or independent holdout. A separate six-article comparison reused the saved October 18 HTTP bytes and previously reviewed sources.

The fetch probe initially used Markdown. A streaming consumer re-extracted the saved HTML as formatted text, then submitted full-body decision spectra for bodies passing the existing size/word/unavailable checks. The three existing headline topic questions were adapted to article evidence in the experimental harness; the subject/emotion/movement cutoffs remained 55/15/80. This was an isolated component pipeline, not an activated single queue job or a daily publication run.

| Measurement | Result |
| --- | ---: |
| URLs with a terminal fetch outcome | 100 / 100 |
| Successful HTTP responses | 85 |
| Fetch failures | 15: twelve 403, one 402, two other failures |
| Parsed bodies | 81 |
| Bodies passing size/word/unavailable checks | 72 |
| Full-body subject decision requests | 90, zero transport/validation failures |
| Articles nominated for at least one character | 65 |
| Subject-character nominations | 113 |
| Native fetch plus initial parse wall time | 401.74 seconds |
| Native fetch/parse process peak RSS | 34.11 MiB |
| Formatted extraction median | 3.69 milliseconds |

The 72 mechanically accepted bodies include known bad extractions. They are **not 72 verified usable stories**. Wall time includes Google resolution, domain spacing and publisher waits; the model ran separately on Archbox. This is not a controlled speed comparison with the browser run on another host/date.

All six previously reviewed articles fit intake limits and retained the reviewed reporting, attribution and late qualifications. Sports Illustrated paragraph duplication from the older custom extractor was absent. Page furniture remained, including a hidden tracking-text block on St. Pauli.

## Memory and shared delivery

A long-lived process extracted the six saved pages 6,000 times in 14.82 seconds, peaking at 15.75 MiB RSS. A broader run extracted all 85 HTTP pages 100 times: 8,500 parses in 28.86 seconds, peaking at 32.17 MiB RSS. Its warm first-quarter peak was 30.31 MiB and last-quarter peak 32.17 MiB. These short stress runs support further testing; they do not establish all-day durability.

The experiment retained one selected body per admitted article: 65 bodies for 113 delivery edges. Decision and plugin inputs were checked against the exact body bytes. Offline delivery replay made no fetch or model calls. This verifies experimental fan-out, not a new production cache implementation, alias deduplication, crash recovery or exactly-once external reads. The earlier production read-once patch remains local and undeployed.

## Actual plugin outputs

Seventeen routed requests exercised the current Journalist, Influencer and Insider assemblers/options/parsers using the local Qwen3.5-4B Q5_K_M model, thinking off, temperature zero, 32,768 context. All 17 prompts contained the exact extracted body. Sixteen responses parsed; one Insider response was rejected because its evidence quote changed an apostrophe and was not an exact source substring. Two Journalist responses abstained.

Source review found material failures despite parser acceptance:

- Yahoo Pacers and 76ers responses lacked their main stories. The extractor returned captions or recommendations. A Pacers report was largely headline-only; the Influencer turned recommendations into a misleading 76ers reading.
- The Marcus Garrett RotoWire result contained legal/gambling footer text. The Influencer invented a mutual split and feelings that the supplied material did not establish.
- A Packers photo caption produced a false Jets admission, which the Journalist declined. The Packers summary itself swapped injuries and extended a season-ending absence to an additional player, although the extraction contained the correct details.
- Historical Nets context produced a false admission and an off-target summary. The Coventry Journalist invented a 5–0 Nottingham Forest result; the extracted text correctly described a 4–1 Newcastle match.

The six broader-batch plugin cases were the first two nominated subject cases per character in sorted case-ID order. They were not chosen for good outputs. Broad Insider cases lacked canonical co-mention candidates, so those cases exercised prose with structured findings disabled. Full-day aggregation, history, production commit/publication and automatic retries were not tested.

Restricting Readability to exactly one `<article>` blocked the three recommendation/footer failure examples, but only 10 of 85 HTTP pages and two of the six reviewed pages passed intake. That variant was rejected because it discarded too much legitimate material.

## Next implementation boundary

Keep the proposed single intake job: resolve/fetch/extract once, score the returned body for each canonical subject, retain one relevant body plus its routes, and let selected characters consume it. Rejected URLs need a small processed/rejected record so continuous sweeps do not repeatedly read them. Study artifacts retain rejected material for audit; production retention policy was not changed.

Before integration, qualify a fail-closed main-article evidence contract that distinguishes actual reporting from a page whose story is absent. A word floor, topic spectrum and syntactically accepted character output are insufficient. Keep the existing evidence-quote guard, and assess downstream factual errors separately from missing or contaminated reader input.

The [machine-readable receipt](../fixtures/classifier/smoothie-intake-2026-10-19.json) contains exact settings, metrics, limits and harness hashes. Private raw HTML, decisions, source text, full plugin outputs, memory samples, source comparison and the pinned experimental Cargo lockfile are in `/private/tmp/scoracle-smoothie-20261019`, with a source/evidence archive alongside it. Temporary inference, PostgreSQL and SSH tunnels were stopped; production services were unchanged.

## Strict-policy rejection exploration

The next offline experiment reused all 85 fetched pages and the six reviewed saved responses, with no new network reads or inference calls. The smallest candidate is the library's `parse_with_policy(ParsePolicy::Strict)` followed by the existing 80-word body minimum and 24 KB maximum. Count extracted body words, excluding the RSS title and metadata. Stop on insufficient output rather than trying more permissive extraction policies. The [library contract](https://docs.rs/dom_smoothie/0.18.2/dom_smoothie/struct.Readability.html#method.parse_with_policy) confirms that ordinary `parse()` tries strict through raw policies, whereas `parse_with_policy` makes one attempt.

On identical HTML, default parsing passed the body checks on 72 of 85 pages; strict parsing passed 45. All three previously failing recommendation/footer examples were blocked, and all six separately reviewed articles still passed. The built-in probably-readable flag marked all three bad examples readable, so that flag alone is insufficient.

Targeted review of the 27 changed outcomes found 25 recommendation, footer, caption or listing outputs, one genuine short brief below the existing word floor, and one substantive report missed by strict parsing. Nineteen changed outcomes were Yahoo pages whose article containers had no paragraphs; strict output contained only roughly ten words of date/read-time metadata. The Big Lead's Commanders injury report is the known false rejection: default parsing returned its substantial report, while strict parsing returned no body. These are development cases, not an independent precision/recall estimate; the 45 passes are not all certified complete fresh stories.

The RotoWire Marcus Garrett example needs a distinction: the raw response does contain a genuine 32-word structured `articleBody`. Default parsing picked the legal footer; strict parsing recovers the brief and some promotion, totaling 54 words. Rejection under the present policy is therefore **insufficient body length**, not proof the article is absent. Structured body metadata can help establish provenance, but a title, byline or `NewsArticle` type by itself does not supply a body.

The proposed runtime outcome is a small acquisition disposition such as `no_usable_article_body` or `insufficient_body`, with no retained article body or character dispatch. This is a read failure/skip, not a negative relevance judgment. Reuse the existing terminal acquisition records, tied to input and reader-policy revision, so ordinary sweeps do not reread the same failure. Transient transport failures retain their existing bounded retry behavior. Keep this within the single intake job; no publisher blacklist, custom site selector, extra model or browser fallback is proposed here.

The [rejection receipt](../fixtures/classifier/smoothie-rejection-2026-10-19.json) records all changed outcomes and the accounting assertions. Private probe source, pinned Cargo lockfile, per-page DOM/structured-body observations and both parsing results are in `/private/tmp/scoracle-smoothie-rejection-20261019`. The schema-body self-test and frozen failure/positive assertions passed. Production code, dependencies, models and services were unchanged.

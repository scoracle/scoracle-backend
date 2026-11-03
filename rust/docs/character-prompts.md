# Character prompt ownership

Plugins own meaning and behavior. Shared code owns assembly and field-presence rules. Every context provider returns its data together with a guide explaining how that data relates to the target.

| Change | Owner |
| --- | --- |
| Role, voice, requested product, output instructions | Plugin `character_instructions.rs` |
| Which evidence is selected and how it relates to the target | Plugin `fresh.rs` |
| Canonical identity and stored attributes | Shared `tools/meta.rs` |
| Connecting context providers, schema, generation settings, version | Plugin `request.rs` |
| Ordering, conditional definitions, serialization | Shared `tools/prompt.rs` |
| Output parsing and evidence checks | Plugin `parser.rs` and existing schema helpers |
| Registration, execution, publication | Existing plugin manifest and runtime modules |

There is no separate voice file or string substitution for voice. Role and expression are ordinary text in the task. Detailed output rules remain system instructions; the task appears once in the JSON. Request order is `instructions → target → new_content`. `instructions` contains the task followed by readable target and content guide lists. The JSON is indented in the actual model input, not only in previews.

Read each payload as a work brief. A reader unfamiliar with the sport should be able to answer: What must I write? Who is it about? Which supplied text supports it? Put the requested output, length, tone, attribution and evidence limits in the task; put field meanings and their relationship to the subject in the context guides. A smaller token count alone does not establish clearer instructions or factual correctness.

## Context contract

`Context<T>` contains typed `data` and a `Guide`. A guide has two explicit parts:

```rust
Guide {
    relationship: "Reports concern the target; their text identifies who speaks and acts.",
    fields: &[
        ("reports.publisher", "supplying outlet, distinct from the speaker."),
        ("reports.headline", "publisher headline; not full article contents."),
    ],
}
```

Relationship guidance is always included. Each `(field_path, meaning)` pair is included only when that path has a supplied value. Paths use serialized field names, are relative to that component's data, and traverse arrays automatically. Zero and false count as values. Missing, null, blank strings and collections containing no supplied values do not trigger a field definition. Values are preserved in the data; filtering definitions does not discard or truncate evidence. The common metadata guide explains missing/null and empty-collection semantics.

Keep the guide beside the evidence projection it describes. A renamed serialized field needs its guide path updated and a rendered fixture checked. Prose punctuation has no program semantics: the assembler no longer parses Markdown definitions.

Use shared metadata when its identity semantics fit. A plugin can supply a different typed metadata component without changing the assembler. Both metadata and fresh components arrive prepared; the assembler has no database-record dependency and never loads or selects evidence. Keep the two component slots until an actual product needs another one.

Share a component or definition when its meaning is identical across consumers. Target/related-entity roles, publisher/speaker roles, stored affiliation/fresh reporting, event dates and target sample/comparison cohort must remain explicit. Avoid instructions that assume every target is a team or player. Read canonical identity and optional attributes from the payload.

The decision model uses the same context ownership convention but retains its native scoring questions and transport. It does not need a prose voice or a character output schema.

## Add or tune a plugin

Use an existing character as an example. Define its instructions, return `Context<T>` from its evidence provider, and connect the providers through `request.rs` to `tools::prompt::assemble`. Implement its output schema/parser and register it through the existing manifest/runtime and evaluation task registration. No new registry, inheritance layer or template language is required.

Keep selection and interpretation in code/text owned by that plugin. Keep reusable ranking, identity and presence rules in the existing shared helpers. The temporary ten-article cap applies to all article-consuming characters: Journalist (`subject`), Insider (`movement`), Influencer (`emotion`) and Scout (`availability`). Keep `tools::source::CHARACTER_ARTICLE_LIMIT` and SQL `headline_entity_cap()` aligned. Frozen-source exclusions retain auditable selection receipts; unserved sources must never be acknowledged as model-used. Analyst and Oracle consume structured evidence, and the decision model scores one article per request. Schema, request and parser must all refer to that same selected order.

Insider and Influencer use evidence-led paragraphs, not a total sentence count. Their existing `body` string preserves blank lines. Influencer writes the reading first, assigns one overall vibe score, then writes one hook; the score and hook describe the whole reading. Both retain a bounded 12,000-character body and reserve up to 3,500 output tokens for the ten-article packet. Those ceilings are safeguards, not length targets. Source/finding validation and the 140-character hook limit remain.

Article IDs, URLs and publication timestamps stay in internal source records and replay receipts, but are omitted from the writing payload. Journalist articles use request-local integer `id` values (1–10); each output paragraph cites them in `report_ids`. Journalist returns `paragraphs` first and one `headline` last. The native schema preserves this generation order. Each paragraph is checked against its own sources; the hook is checked against the union of cited sources. The existing product stores one recap with blank lines between paragraphs, one hook, and deduplicated internal source IDs. These handles map back to the selected internal sources and are not persistent article identities. Insider keeps its existing report indexes. Old Journalist replies using `report_keys` belong to the archived contract; the current parser does not accept both forms. Dates within source wording are preserved, as are stored-attribute observation dates and measurement windows. This presentation rule does not change article selection or assert that all queued reporting was published today.

Export a complete current request without a database or model call:

```sh
cargo run --offline --example character_package -- fixtures/quality/narratives/one-result.json --request
```

This displays the exact system and user inputs, schema, options and should-call decision. Structured fixture parts rebuild through production assembly. Inspect the selected evidence, included guide, complete request, raw output and validation result together. Saved input hashes, prompt versions and native token receipts identify what actually ran.

For structural changes, compare complete requests byte-for-byte and run `cargo test --offline --lib --examples --bin eval`. For instruction changes, update every affected plugin version (including consumers of a changed shared guide), regenerate affected fixtures and compare identical evidence using complete native input tokens and manual source review. Keep failed outputs and receipts. Parser acceptance alone does not establish factual correctness.

The [direct-instructions trial](../fixtures/characters/direct-instructions-2026-11-02/README.md) records the plain Journalist, Insider and Influencer briefs, numeric Journalist citations, and its single-hook recap contract. The [clear-payload trial](../fixtures/characters/clear-payload-2026-11-02/README.md) records the preceding formatting comparison and three-character replay. The [context-component trial](../fixtures/characters/context-components-2026-11-02/README.md) records the preceding structural comparison. Historical experiment folders preserve the code versions and prompts they tested.

## Banked for the next prompt version

User-approved direction, November 2, 2026. Implemented and tested November 3 as experimental Journalist, Insider and Influencer candidates; not deployed. The [48-call Mac-only trial](../fixtures/characters/search-themes-2026-11-03/README.md) records mixed quality results and the unchanged production pause. The approved wording below remains the design intent; the trial uses a concise conditional version to avoid asserting Google provenance for unknown or legacy inputs.

Search provenance and evidence limits:

> These headlines came from a Google search for the target. Search results can include unrelated stories, repeated coverage, speculation and clickbait. Appearing in these results does not establish relevance or confirm a claim. Focus on clearly relevant stories, preserve uncertainty, and do not turn a headline's question or implication into a fact.

Recurring themes without forced connections:

> Look for recurring themes across relevant headlines. When several distinct reports support a common story, summarize it as a prevailing theme. Repeated copies of one headline are not independent support. Keep unrelated stories separate, and do not force a shared narrative when the evidence is thin.

Influencer-specific distinction:

> Distinguish the tone of the coverage from evidence of how people actually feel.

Sensational wording alone should not establish strong public emotion. Describe Google-search provenance only for evidence actually collected that way; other payloads retain their own provenance. Keep these meanings with the relevant evidence guides, preserving each character's task and voice.

The first trial compares both additions across eighteen saved cases, then isolates search/noise and theme guidance on six of them. Continue comparing identical saved targets and evidence from the [November 2 corpus](releases/2026-11-03-early-corpus/README.md). Measure complete native input tokens and manually compare relevance, uncertainty, actor attribution, separation of unrelated stories and unsupported emotion claims. Retain failures and receipts, and update affected prompt versions and fixtures for each subsequent candidate. Parser acceptance alone is not a reason to deploy.

The subsequent [empty-teaser refinement](../fixtures/characters/substantive-news-2026-11-03/README.md) tested stronger filtering language, with over-filtering and retained teasers still observed. The user then removed the sentence containing teaser examples and shortened the instruction to keep specific information. Current experimental versions are `n140-specific-news`, `insider-source-v41-specific-news` and `vibe-frame-v53-specific-news`, with this passage:

> Some articles match the target but reveal no news. Keep specific news, opinions and reactions. Do not invent the missing details or turn a question or implication into a fact.

The two requested cuts were applied to all three guides and current fixtures, then tested in [eight Mac-only comparisons](../fixtures/characters/substantive-news-2026-11-03/specific-news-results.json) at the user's request. Input tokens fell 2.48% versus the preceding wording, but manual quality results remain mixed and reliable filtering is unproven. Earlier trial results describe their recorded versions. Production remains paused.

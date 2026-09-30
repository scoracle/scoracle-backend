# Influencer: parts and assembly manual

Baseline revision: `9ade4b79`. Active prompt: `vibe-frame-v3`. Local changes;
not deployed. The September 29 user clarification supersedes the earlier
abstention/classifier discussion.

## Binding design

The plugin supplies the complete world, the parts and the assembly manual.
The LLM is the cognition engine only: synthesize and articulate the emotional
charge around the subject using those supplied parts. It has zero world-building
responsibility. Harvester owns intake selection and its remaining calibration
noise. Do not add a second reaction gate, classifier, content-policing layer or
correction loop inside Influencer.

## Active components

- Shared `plugins/meta.rs`: who the subject is—canonical name, entity type and
  sport. Durable identity stays in provenance.
- `influencer/cognition/fresh.rs`: intact publisher context using shared
  `support/source.rs`, plus the plugin's source-headline title policy.
- `influencer/memories.rs`: dated earlier reporting, selected through the shared
  Postgres/DuckDB study. Seven days before fresh publication, at most two reports
  and 1,200 bytes. Unknown publication time supplies no historical study.
- Shared `support/form.rs`: nullable body, one paragraph per observation,
  140-character paragraph ceiling and 1,200-character body ceiling. The shared
  decoder checks transport and the requested dimensions.
- `influencer/cognition/influencer.rs`: emotionally attentive, clear, concise
  voice with care and restraint.
- `influencer/cognition/prompt.rs`: identifies each supplied part and instructs
  synthesis of fresh and relevant history into their combined emotional charge,
  preserving speakers, timing, scope and uncertainty.

Production and evaluation assemble the same package and make one cognition call.
Sentiment remains unknown and persists as NULL. A nullable body is a transport
capability, not a required emotional-eligibility classification. Source freshness,
input bounds, source receipts, claim fencing, atomic product/disposition writes
and completion outbox remain ordinary application mechanics.

## Pruned

Removed separate reaction prompts/corrections, packet palette and loaders,
previous-score fallback, obsolete scored-memory mission, unused packet-block
renderer and its budget machinery. Scout and Insider retain their live source
slices, attribution, contradiction markers and framing. Historical data remains.

Influencer now uses the shared structural parser directly. The local parser and
its product-name/foreign-script checks are removed. Its evaluation adapter no
longer runs abstention, keyword, synonym, word-count, sentence-count or numeric
score expectations. The replay helper checks form and completion only. Current
quality cases describe the synthesis task without expected pass/card labels.

No additional tools, classifiers, palettes or model calls were added. Earlier
v1/v2 replay results and `articulation-v2-probes.jsonl` remain diagnostic history;
their old disposition verdicts are not current product requirements. Recorded
factual additions remain factual additions, not reclassified successes. The v3
manual has not been live-replayed; no claim of improved model fidelity is made.

## Clearly limited input

The shared tool now lives at `plugins/memories.rs`, alongside `meta.rs`.
Reporting and match-statistic adapters share one DuckDB runner. Influencer keeps
its seven-day/two-report/1,200-byte request policy locally; dated-report selection
and budgeting use the shared `ReportingHistory` implementation. Journalist and
replay callers also use this tool.

Historical memory supplies only reported headlines, publishers and dates.
The shared reporting study has no source-passage field. Speaker details and
qualifications present only in those passages are therefore unavailable to the
cognition engine. If fuller emotional continuity is needed, supply bounded
historical source passages as a memory part. Do not ask the model to reconstruct
missing material. This is an input limitation reported to the user, not a new
classifier or an implemented memory expansion.

## Remaining consumers and next window

Analyst already accepts prose with optional sentiment. Oracle's current
`SynthVibe` requires a numeric sentiment and omits this scoreless pillar; its
later window owns that consumer change. Historical sentiment studies retain
historical measured scores; NULL must not become 50.

Shared packet/Editor and generic memory infrastructure remain for other live
consumers, with retirement owned by their later windows. Continue with Scout in
a fresh context using the same parts-and-manual design. Do not reopen the retired
Influencer abstention/classifier decision.

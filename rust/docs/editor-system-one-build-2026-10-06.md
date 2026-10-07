# Harvester, Editor and model ownership

Cheap code owns collection and compilation. Cheap System 1 inference scores explicit questions. Expensive text generation owns supported discovery and articulation.

Harvester screens headlines for possible entity relevance. Editor acquires and reads the retained article, scores entity relevance and downstream suitability, and queues source references. Journalist, Influencer and Insider read actual publisher language. Editor never produces an editorial summary, tone verdict, inferred motive or outline.

Each plugin's prompt module owns its requested model capability/route and inference settings. A model capability is itself a plugin; application registration binds it to a deployment. Model selection is request metadata, not text inside the model's world. Endpoint addresses remain deployment configuration. Actual model/checkpoint/settings and exact source coverage are recorded with the result.

Routing scores select work; they do not establish emotional tone or factual truth. Thresholds remain provisional until evaluated against labeled corpus cases. Full article access preserves attribution, quotes, qualifications and source identity. Incomplete reads fail explicitly rather than masquerading as complete reads.

## Build ledger

- Starting point: Harvester currently owns headline scoring, acquisition, opening-only theme scoring and publication; Editor has no active worker. Existing typed System 1 transport and claim-fenced publication will be reused. Legacy editor_reads packets remain historical and are not revived.

- Built separate Harvester and Editor queue claims. Harvester no longer has a web-fetch grant. Editor owns publisher acquisition and the typed model call for article relevance plus seven downstream predicates. Existing immutable receipt tables and release controls are reused; legacy editorial packets remain unused.
- Full retained article coverage is explicit: 100-word / 1,200-byte windows, at most 64. Every window retains offsets and model/checkpoint/coverage provenance. Relevance below 0.5 prevents that window from contributing theme scores. Article relevance is an admission signal, not proof of truth.
- Shared reader supplies complete verified retained text to Journalist, Influencer and Insider; Scout retains the opening excerpt. The three article consumers request a 32,768-token context envelope and bounded packages. Oversized input fails visibly; no truncation or quote cherry-picking is added.
- Model routes moved into prompt.rs for all seven generative consumers, preserving existing route/environment identities through manifest aliases. System 1 and text-generation binding live in model plugin modules; application registration supplies endpoints.
- Isolated local Postgres integration checks passed for headline rejection, selective routing/release controls and full downstream retry/idempotency/source-change fences with the actual DuckDB helper. No production queue or historical rows were changed.
- Read-only Archbox corpus replay: Steelers 853552 scored over 10 windows and routed Influencer; Jazz 852844 over 4 windows routed Journalist/Influencer/Insider; Sunderland 856275 over 3 windows was incorrectly rejected; unrelated Frosinone/Messi 859022 over 23 windows was incorrectly admitted and routed Journalist. These are calibration failures, not evidence that provisional thresholds are accurate.
- Full-source Influencer replay recovered additional Steelers quotations and qualifications but still misattributed Chris Rigg as a writer and generated Messi content for the unrelated subject in historical pending work. The old 140-character paragraph validator rejected all four outputs; this does not measure factual quality. Private receipts: /tmp/scoracle-output-tuning-20261006/editor-reader-routing.jsonl and influencer-article-reader.jsonl on Archbox.
- User steered form simplification: retain a 140-character header and concise body paragraphs. Removed paragraph ceilings from the shared form, parser and plugin instructions. Taking “all we need” literally, removed the old 1,200-character body cutoff too. Header validation, nonblank/schema/source guards and model generation token budgets remain. No truncation or automatic paragraph splitting was introduced.

- Final concise-body replay: all four historical pending cases passed the structural parser. This is not a fidelity pass: Jazz ends mid-thought, Sunderland retains a speaker-role error and Messi remains unrelated to the supplied subject. Current private local context example: /private/tmp/scoracle-output-tuning-20261006/influencer-context-853552-reader.json.
- Final checks: 376 unit checks passed, 4 relevant isolated database integration checks passed, all targets compile, whitespace checks clean. The build remains undeployed and routing calibration is still open.

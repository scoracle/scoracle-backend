# Plugin-owned output contract and local model comparison — September 26

## Contract

Studio hosts inference and validates the result. A publishing plugin decides which facts and interpretations can appear before inference. The model may choose among plugin-approved phrasings; its response is never copied into served prose. `src/studio/palette.rs` is the reusable finite-output contract: one positional phrasing choice for each selected fact, with exact cardinality and bounds validation. Invalid output fails the work item. The schema also constrains decoding where the provider supports it.

Scout is the first migrated publisher. Its adapter selects a palette from identified, ranked measurements. It chooses the top two eligible percentiles and the lowest eligible percentile, preserving the source measurement name, quality band, percentile, and eligible population where known. A compatible prior-season comparison adds its arithmetic percentile direction, and a prepared recent-form trend adds a separate statement. Unknown ranks and measurements with no source identity cannot become measurement claims. A profile with only a valid composite score gets a composite-only statement; a profile with neither produces the existing no-stats marker. The model chooses the approved phrasing for each statement; Studio assembles the body and the plugin supplies the headline. Scout output contract is `rating-commentary-v7-palette`, prompt version `s60`.

The former Scout card parser and s59 quality fixtures remain as archived diagnostics. The old `rating` evaluation task still replays that contract and must not be interpreted as a production s60 quality gate. Editor and Harvester were untouched. Other publishing plugins still serve generative prose and need plugin-owned output plans of their own; this implementation does not claim that they are grounded by the palette.

## Remaining publisher migrations

| Plugin | Material its output plan must own |
| --- | --- |
| Influencer | Attributed article claims, sentiment scope, and uncertainty before any mood wording |
| Journalist | Source-linked developments and the exact article IDs that support each narrative |
| Insider | Verified relationship and transaction states with source attribution |
| Analyst | Selected Scout and Influencer findings, with their dates and null state retained |
| Oracle | Selected claims from all finished character products and the derived score inputs |

Each needs a plugin-specific claim selector and approved expression before its raw model prose can be retired. Harvester's classification and copied source text do not make downstream claims automatically publishable. The ongoing Editor to Harvester replacement is independent of this output boundary.

## Matched local probe

`examples/palette_model_compare.py` sends the same four synthetic palettes (one through four facts), schema, temperature 0, 4096 context, 160 output tokens, and thinking disabled to each model on an isolated Ollama server at port 11435. There are three repeated calls per case. The fixture statements are synthetic; this is a contract and latency probe, not a sports editorial quality test or a production database replay. Raw replies and rendered text were captured at `/tmp/scoracle-palette-model-compare.jsonl` for this session.

Local model blobs: Granite `sha256:e04066639658…`; SmolLM3 `sha256:048b986bb243…`. SmolLM3 was installed as the community Ollama package `alibayram/smollm3` for this experiment; the production route was not changed.

| Model | Valid compositions | Median warmed call | Mean output tokens | Style observation |
| --- | ---: | ---: | ---: | --- |
| `granite4.2:3b` | 12/12 | 0.716 s | 24.5 | Mixed phrasing choices on multi-fact cases |
| `alibayram/smollm3` | 12/12 | 0.672 s | 20 | Chose the second phrasing for every fact |

The initial ID-based version let both models repeat one rich-case fact and omit another, despite valid JSON. The positional schema removed that failure in this small probe. The 0.044-second median difference is too small and too environment-specific to justify changing the production model route. The finite palette blocks model-invented stats by construction, but it currently produces plain, repetitive Scout prose. Improving the plugin's approved expression and evaluating it with real prepared assignments are required before a model switch.

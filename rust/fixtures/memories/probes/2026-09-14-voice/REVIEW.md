# Scout character comparison

Two matched seeds compare s33 with a candidate s34, keeping the full v2 Rogers
context, form and runtime settings fixed. The candidate directs attention toward
sporting contribution and distinguishes source coverage from actual playing time.

| Voice | Seed | Prompt tokens | Body characters | Parser |
|---|---:|---:|---:|---|
| s33 | 17 | 1,520 | 2,283 | Fail |
| s34 candidate | 17 | 1,590 | 1,163 | Pass |
| s33 | 43 | 1,520 | 1,086 | Pass |
| s34 candidate | 43 | 1,590 | 2,472 | Fail |

**All four rejected editorially; candidate not adopted.** Both voices invent
transfer/competition dates. The candidate's seed 43 still invents limited scoring
ability, physical/work-rate implications and an expectation of one assist per
appearance. The schema-valid candidate seed 17 mainly narrates context metadata.

Matched seeds did not reproduce the earlier s33 outputs exactly; raw responses
are retained, so no deterministic GPU claim is made. A voice-only edit is not a
fix for mixed clock semantics or incomplete underlying coverage. Subsequent work
adds matched production to the shared memory adapter; that newer adapter was not
the input of this experiment.

The exact candidate is archived in `scout-s34-candidate.rs`; the runtime retains
s33. No routes, production records or corpus changed.

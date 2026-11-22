# Typed metadata and causal diagnostics — 2026-11-01

**Typed fields do not resolve metadata contamination. Removing background also
does not resolve source interpretation errors.** This is a second, explicitly
separate experiment after the [24-call layout matrix](../context-boundaries-2026-11-01/README.md)
failed its selection criteria. Neither typed candidate qualifies for follow-up.

## Same attributes, different representation

The existing metadata loader formats database attributes as a prose record.
The team coach subquery chooses the newest `persons.created_at` row matching
team/sport and `kind='coach'`; that query does not validate current coaching
status. This is a data-quality limitation separate from whether the writer
preserves a supplied source actor. No database values or loader behavior were
changed here.

A read-only query retrieves exactly the columns used by that formatter. The
builder proves that each typed object reconstructs the original served record
byte-for-byte, including team name, league/division, venue/city and coach name.
It does not infer missing fields, parse prose heuristically, or replace database
attributes with externally researched facts. Fresh source objects, tone and
source provenance remain identical. Only `database_record` changes from a string
to an object with named fields.

Twelve candidate calls compare typed records in fresh/meta/tone order and
meta/tone/fresh order, holding the previous examples system and statement schema
fixed. The six controls from the immediately preceding matrix are reused to
avoid six redundant calls. Both typed candidates yield **3/6 clearly faithful
packets**, versus **4/6** in that control. These are manual source-fidelity and
readability judgments, not parser scores or general accuracy estimates.

- Typed source-first produces **“Filippo Pavlovic”**: an unsupported first name
  matching the metadata coach, combined with the source surname. The unexplained
  amount still becomes a valuation. A recall title still assigns shared injury
  concerns to actors with different roles.
- Typed context-first preserves Pavlovic but loses the two required return
  factors, imports facts between publishers and invents a Thanksgiving streak.

## Deliberately altered metadata diagnostics

Three additional calls hold the original Milan reporting and tone fixed while
changing metadata. These are **counterfactual diagnostics, not real-metadata
quality trials or production candidates**.

| Background intervention | Speaker output | Other defects |
| --- | --- | --- |
| Coach name replaced with invented Maren Vale | Pavlovic retained; title says coach comments | Invented valuation and imported goalkeeper role |
| Coach name replaced with invented Tomas Reed | Pavlovic retained in body/title | Invented valuation |
| Database record removed; subject identity retained | Pavlovic retained | Invented valuation, imported match details, unsupported tactical title |

Neither invented name is copied. Changing only coach-name content nevertheless
changes attribution relative to the control, which says Filippo Galli. This
supports a causal effect of metadata content for this fixed prompt/source packet;
it does **not** demonstrate a general always-copy-the-coach mechanism or universal
behavior. The hybrid-name result further shows that JSON typing is not a reliable
information boundary for this writer.

The record-absent diagnostic still invents what the amount means and attributes
neighboring-source details to the wrong report. Thus metadata contamination and
source interpretation/ownership are separate unresolved problems. Dropping
metadata is not a complete fix and was not adopted.

[Protocol](protocol.txt) · [Selection](selection.json) · [All reviews](review.json)

## Cost, integrity and stopping decision

| Run | Calls | Input tokens | Median wall time | Range |
| --- | ---: | ---: | ---: | ---: |
| Typed source-first | 6 | 4,933–5,349 | 6.24 s | 5.79–9.74 s |
| Typed context-first | 6 | 4,932–5,348 | 6.26 s | 5.89–10.16 s |
| Counterfactual diagnostics | 3 | 5,312–5,335 | 9.34 s | 6.98–9.87 s |

All 15 calls have complete native input, EOS, no truncation, fixed model identity
metadata and valid source mappings. Source mappings do not establish truth.
The 4B, temperature 0, thinking-off mode, 7,900 context and 2,100 output reservation
are unchanged; no added inference stage is proposed. Total call wall time is
**112.284 seconds**, including all diagnostics. Warm-prefix cache reuse was
4,610–4,819 tokens. [Measurements](measurements.json).

No candidate met the predeclared follow-up criteria, so no repeats or new-subject
qualification calls were spent. Liverpool, Boston and Green Bay remain frozen
and unused. Production prompt, metadata loader, guards and database are unchanged.
Daily capacity and manual group-preparation cost remain unmeasured.

`attributes.sql` and `attributes.jsonl` retain the read-only projection;
`build_inputs.py` asserts lossless record reconstruction; `run.py` performs
admission and model-identity checks; `analyze.py` verifies exactly what changed
in typed and diagnostic inputs, source mappings and complete receipts. The
subsequent [production field-order check](../production-order-2026-11-01/README.md)
closes the recursive-sorting difference in the experimental fixture builder.

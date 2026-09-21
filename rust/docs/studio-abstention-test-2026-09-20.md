# Scout abstention test: Ornith 9B

Tested s52 on `ornith-1.5:9b` alone, using the current evaluation/production system instruction and card-or-null schema. Temperature 0, thinking off, context 4096, output budget 700. Exact requests and raw responses are in `logs/studio-abstention-20260920/`, with the Rust request builder and Python runner. The sparse and rich cases use the existing fixture evidence unchanged; the unknown case replaces zero goals with unmeasured goals and the games sample with unavailable.

These are first-generation diagnostics, before production parser validation and rewrites. All three returned parseable card JSON with `stop`; none returned null. This does not constitute three successful production cards: sparse and rich exceed the 1200-character surface limit and would need correction.

| Case | Observation | Body characters |
|---|---|---:|
| Both measures and sample unknown | Writes an explanation of why it has no supportable claim instead of passing; assumes goals and expected assists are expected midfielder contributions | 848 |
| Zero goals, unknown expected assists | Preserves the zero/unknown distinction, but produces an extended gap explanation and calls 25 games a full campaign | 1565 |
| Rich profile, rim protection at percentile 96 | Says “elite company,” not best in league; nevertheless converts rim protection into blocks, invents defensive sequences and turnover causes, and demands a full offensive profile | 1951 |

The unknown case ends: “The card is empty of supportable claims, so there is nothing to say about Reed's abilities, tendencies or standing.” This is especially clear evidence that the model recognizes the lack of a substantive claim but still produces a card about that lack.

The rich case invents “3.2 blocks per game” from `rim_protection`, and attributes turnovers to holding the ball too long or forcing decisions. It also calls the gap “the only thing standing between a full read and this one,” despite the shared partial-profile language.

A separate compatibility check retained the model, generation settings and identical schema, replacing the messages with a direct request for JSON null. It returned exactly `null` with `stop`. Therefore the server/schema permits abstention; the observed failure occurs with the actual Scout assignment. This control does not prove which instruction causes it.

> Later source audit: repository SQL derives NBA Rim Protection from blocks, while this synthetic fixture does not preserve that definition. The blocks interpretation is unsupported by the fixture, not established as false for live NBA data. See [pressure audit](studio-pressure-audit-2026-09-20.md).

## Percentile concern

The exact “best in league” error did not recur in this sample; one response cannot establish it is fixed. Percentile 96 is high relative standing on the named measure in the stated eligible population. It does not establish ordinal rank one, overall superiority, or a different comparison population. Current preparation supplies a quality band (`elite` at percentile >=90) and generic sport/season population language, but no explicit ordinal rank or population size in this fixture. Do not convert a measure's quality band into a whole-player superlative.

## Conclusion

The abstention mechanism is usable but the option alone has not resolved compelled output or unsupported interpretation. The strongest next diagnostic is the conflict between accepting uncertainty as a finding and passing when no meaningful character claim exists. A gap may qualify a supported reading; a card whose sole content is “I have nothing to say” has not exercised the intended pass behavior. Resolve that distinction in the task contract rather than treating this as a model comparison or adding a banned-phrase list.

No production code changed during this test. No production routes or database writes were changed. The isolated loopback model server was stopped afterward.

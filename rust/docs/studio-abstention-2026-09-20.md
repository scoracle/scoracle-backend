# Scout can decline to publish a card

## Contract

Claim selection and story structure are restored in shared `form.rs`, superseding the omission experiment. A card must have a meaningful supported claim. The missing capability was the alternative: Scout's schema required a card, the parser required a nonempty body, and creation rejected the extraction layer's existing `None` result.

Shared form now provides an abstention instruction and a schema wrapper accepting either the existing complete card or JSON `null`. Scout is the first enabled voice. Other voices retain their current output contracts; offering null without handling it through their parsers and publication paths would recreate the mismatch.

A measured zero or observed absence may still warrant a claim. Missingness does not automatically force a pass. The model decides whether its supplied evidence supports a meaningful claim in its own scope. There is no deterministic threshold or scripted conclusion.

## Flow

- Production and evaluation Scout requests use the same card-or-null schema and instructions.
- Exact JSON `null` maps to `None` through both Scout parsers and completes extraction without a retry. Passing also works after a bounded surface rewrite.
- Empty cards, malformed JSON and transport failures remain failures. Legacy plain-text parsing remains unchanged and is not an abstention mechanism.
- Creation returns a called product marked `abstained`, with no body or headline. Model identity, input hash, request diagnostics and prepared analytical observations remain available. This differs from pre-call no-stats and unchanged-input outcomes.
- Existing publication writes a null-body marker and follows normal completion/downstream scheduling. The optional generation ledger records `abstained` and `model_abstained`; ledger insertion remains best effort. No database migration is needed.
- Reader-facing rating already selects the latest generation before checking body presence. Oracle also reads the latest generation. Analyst previously filtered out null bodies before selecting the latest row; it now selects the latest row first, preventing fallback to stale Scout prose after a pass.
- Evaluation recognizes a pass as parsed and displays it explicitly. It requires evidence review instead of granting a vacuous quality pass for producing no claims. This evaluation review flag does not trigger production retries.

Versions: Scout s52 / rating-commentary-v6; restored shared instructions also version Momentum momentum-s32, Vibe v36, Insider is15, Narratives n34 and Oracle or24.

## Validation and limits

Deterministic tests cover one-call abstention with provenance, malformed versus explicit pass handling, passing after a rewrite, marker publication selection, evaluation visibility, and the normal card path. Model comparisons were stopped; no model calls were made for this change. This establishes that the harness permits and handles abstention, not that models will always choose correctly or that hallucinations have been eliminated.

The SQL change was reviewed against existing nullable publication and reader contracts. Database integration tests require an isolated migrated database and were not run. No production deployment, route change or database write was performed.

# Observe content validation in production

The user requested publication of model mistakes to support prompt tuning. `CHARACTER_CONTENT_POLICY=observe` makes content and style validation advisory for Journalist, Insider and Influencer. Strict parsing remains the default when the variable is absent; direct fixture parser checks remain strict for quality comparisons.

The existing validators run first. If a response fails, an opt-in structural decoder can accept it without a rewrite or another inference call. The exact request, response, first strict validation error, claim identity and build commit are retained in the private diagnostic archive with `disposition=observed`. Actual failures retain `disposition=rejected`. This describes parser disposition, not successful database publication. Influencer receipts retain the policy and warning for replay after a publication rollback.

Malformed JSON, absent/blank required content, invalid source indices, invalid score ranges, incompatible transfer status/stage and unresolved canonical counterparties still fail. Database identity, source currency, claim ownership, typed storage and transaction checks remain active. Explicit model abstentions still abstain. Existing failed/held work is not reset or retroactively published.

Prompts, native schemas, model weights, source selection, cap ten, concurrency and required daily stages are unchanged. No schema migration or API change. Old outputs and rejection archives remain intact. Content mistakes can now reach the public frontend; parser acceptance must not be presented as factual accuracy.

## Verification and rollout

Deployed commit `73a9882f62f4` to both writers. Mac restarted at **2026-11-03 00:47:29.920975 UTC**; Archbox restarted at **00:47:40.680765 UTC**. Each startup confirms `observe_character_content=true`. Treat those host-specific times as the experiment boundary; in-flight calls drained under the old policy first.

- Full checks: `cargo test --offline --lib --examples --bin eval`: **429 passed, 74 ignored**. Eight focused observation tests passed after the final diagnostic change. Coverage includes unchanged source mapping, unsupported content bypass, malformed JSON rejection, score bounds including zero, no extra inference, retained warnings, strict default behavior and saved Influencer receipt replay. Collector self-check passed. Two HTTP tests initially failed because the sandbox prohibited local listeners; the complete rerun with localhost access passed.
- Mac binary SHA-256: `a2635c9d3e7dd092fef734da5c3f580c4ba4ced009bf56a518b5f26ed22535cd`. Release and previous launcher: `/Users/scotty/scoracle-worker/releases/observe-content-20261103-73a9882f/`.
- Archbox binary SHA-256: `25122d6d5a3636152c1367588a2774ba23900d4537f51513c265ea9cc8049a08`. Release, previous service, private strict boundary snapshot and verification: `/mnt/data/backup/scoracle/releases/observe-content-20261103-73a9882f/`.
- At 00:48:08–09 UTC, all six Archbox services and the Mac writer were running; both Qwen endpoints were healthy. Ten-article cap and required stages `{narratives,vibe,transfers}` were unchanged. The first post-activation snapshots contained **14 observed Journalist attempts** (Mac 9, Archbox 5). These are validation attempts, not a count of published cards.
- A private Archbox verification at 00:48:43 UTC matched an `unsupported_named_claim` warning response to saved, independently qualified Journalist row 191233 and the public `/api/v1/nfl/player/20/news` body. This verifies the publication path, not factual quality. Raw response/API receipts remain private on Archbox.

The strict and observation snapshots remain private on their originating hosts. Local aggregate checkpoint files remain uncommitted. The monitoring schedule resumes with this boundary and separate observed/rejected counters, ending by 03:00 UTC. Previously rejected/held assignments were not reset or retroactively published. Insider and Influencer use the new policy on their next eligible generations; their queue was already empty at activation.

Rollback: remove `CHARACTER_CONTENT_POLICY=observe` from each writer launcher/service and restart that writer gracefully. This restores strict handling for new generations; it does not retract cards already published during observation.

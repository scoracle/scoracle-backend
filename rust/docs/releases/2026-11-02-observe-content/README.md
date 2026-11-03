# Observe content validation in production

The user requested publication of model mistakes to support prompt tuning. `CHARACTER_CONTENT_POLICY=observe` makes content and style validation advisory for Journalist, Insider and Influencer. Strict parsing remains the default when the variable is absent; direct fixture parser checks remain strict for quality comparisons.

The existing validators run first. If a response fails, an opt-in structural decoder can accept it without a rewrite or another inference call. The exact request, response, first strict validation error, claim identity and build commit are retained in the private diagnostic archive with `disposition=observed`. Actual failures retain `disposition=rejected`. This describes parser disposition, not successful database publication. Influencer receipts retain the policy and warning for replay after a publication rollback.

Malformed JSON, absent/blank required content, invalid source indices, invalid score ranges, incompatible transfer status/stage and unresolved canonical counterparties still fail. Database identity, source currency, claim ownership, typed storage and transaction checks remain active. Explicit model abstentions still abstain. Existing failed/held work is not reset or retroactively published.

Prompts, native schemas, model weights, source selection, cap ten, concurrency and required daily stages are unchanged. No schema migration or API change. Old outputs and rejection archives remain intact. Content mistakes can now reach the public frontend; parser acceptance must not be presented as factual accuracy.

## Verification and rollout

Pending deployment. The last strict-mode snapshots were captured before activation on both hosts. The existing monitoring schedule was paused during the change and will resume with this boundary, ending by 03:00 UTC.

Rollback: remove `CHARACTER_CONTENT_POLICY=observe` from each writer launcher/service and restart that writer gracefully. This restores strict handling for new generations; it does not retract cards already published during observation.

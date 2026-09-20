# Read-error cleanup implementation

Implemented locally from the September 20 inventory. This change does not mutate production,
retry failed work, release held Rating jobs, or call a model.

## Editor request budget

Editor now budgets the complete system-plus-user conversation and preserves both its 900-token
answer allowance and a 128-token chat-template safety margin inside the 4,096-token window. The
old independent 7,200-character article cap is gone. A conservative BPE-style estimator counts
punctuation-heavy and non-English input more aggressively, bounds feed metadata, and selects the
largest UTF-8-safe article prefix that fits.

Regression coverage includes the retained failure class: a punctuation/table-heavy article that
fits below the old character cap but overflows when the full request is counted. A second case
proves oversized source metadata cannot consume the answer reservation.

## Analyst Studio contract

Analyst prompt version `momentum-s28` now receives:

- the latest finished Scout card for the current season;
- the latest finished Influencer card;
- compact identity/schedule memory, without Scout's raw multi-season performance and cohort
  arithmetic; and
- one explicitly dated trajectory study, with separate windows and sample counts for statistical
  form and reported mood.

The prompt no longer receives compact substitutes for the two character readings or an upstream
direction declared "final." It states that the windows are independent, missing is unmeasured
rather than flat, and measured change does not establish cause. Direction and conviction remain
deterministic publication fields derived from the stored momentum score.

The input hash now covers the exact supplied cards, their dates and source hashes, the dated study,
and the selected memory fingerprint. Current momentum quality fixtures were recaptured for the new
contract.

## Scout grounding

The request-aware Scout parser now verifies every numeric literal in the visible card (headline and
body) against the exact prepared assignment. Equivalent formatting such as `95` and `95.0` is
accepted; values absent from the evidence trigger the existing bounded correction path. Regression
coverage rejects the retained invented `3.71`, `0.44`, and `67.1` shape and accepts exact supplied
measurements. Existing direction, percentile-band, unsupported-cause, height, thin-sample, and
measure-association guards remain in place.

## Verification

- `cargo test --all-targets`: 500 library tests passed, 57 isolated-PostgreSQL tests ignored by
  their existing opt-in contract; all binary tests passed.
- `cargo clippy --all-targets -- -D warnings`: passed.
- `cargo fmt` and `git diff --check`: passed.

Production replay and semantic model review remain release gates. Historical Insider/Oracle debt,
acquisition failures, and targeted retry selection still require bounded production inspection;
this implementation intentionally does not infer one cause or retry the aggregate populations.

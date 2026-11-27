# Classified headline chat pilot

Run from the backend experiment worktree on an Apple Silicon Mac with Apple
Intelligence available, macOS 26.4+ and a matching Swift SDK:

```sh
bash scripts/classified-headlines/run.sh            # real iOS reading skills
bash scripts/classified-headlines/run.sh --session  # plain Foundation Models session
```

`IOS_CHECKOUT` can override the sibling `../scoracle-ios-classified-headlines`.
Use iOS commit `f79e5db` to reproduce. No network, database or model download
is part of the test. The JSONL files are observed responses on macOS 26.6.2,
not golden answers. `packet_tokens` excludes instructions, history and output.
Both runs use `SystemLanguageModel.default`. The skill run uses the app's
preparation, admission, answer guard and retries with a frozen `ChatAPI`; it
bypasses routing, networking and UI. The plain session has different
instructions and full session history, so compare packet variants **within**
each run rather than treating differences between runs as a single-variable test.

## What was compared

- **Arsenal:** the backend's November 15 synthetic pipeline qualification.
  The published packet is the captured response through the iOS compactor.
  The source packet uses the three matching RSS fixture headlines, publishers
  and their shared publication time. The focused case manually selects the
  report about the named player: an upper bound on retrieval, not a working
  retriever. Mood and transfer cases keep the relevant product measurement
  or stage alongside the cited headline.
- **Chelsea:** the October 20 archived 24-report journalist input. Earlier
  and latest are a **publication-time replay**, not proof of when ingestion
  finished. Only one headline appears after the earlier cutoff. It reports
  an unnamed signing; a separate outlet reported interest in Yassine.
  The scored variant adds rounded movement and subject scores from that capture.
  The scoped variants add prose explaining evidence limits.

## Decision from this run

Pilot journalist attribution first. Use the backend to select subject-bound
reports; send the exact headline, publisher, publication time and stable source
reference together. Keep scores on the server for selection until a particular
user question needs a named measurement. The same source reference can render
an exact quote or link without asking the model to reconstruct it.

The published Arsenal control assigned another player's injury to Sample.
Three source headlines improved attribution but still produced a false signing.
One focused headline gave the correct fee agreement and exact citation in
90 packet tokens (386 for the published control), but still turned lack of
confirmation into a categorical negative. Vibe preserved the positive reading
and quoted the publisher rather than Scoracle's own headline. Transfer still
asserted a signed contract in every variant. Extra scope prose didn't fix it.

Chelsea provides the harder counterexample: the later unnamed signing confused
attribution, and adding scores increased the packet from 1,236 to 1,630 tokens
and produced an unsupported identity match. Every skill variant accepted a
Stamford Bridge departure based on a **Berekum Chelsea** headline. The plain
session also invented signing odds, even with no scores present.

Before changing the default, replay more real entity-days with independently
checked subject matches and reports that disagree or contain no answer. Judge
source attribution, identity, event status and unsupported claims separately.
For transfer confirmation, prefer an explicit recorded status or “not confirmed
in this reading” over a generated yes/no. These fixtures test feasibility and
failure modes, not prevalence, production retrieval quality or iPhone latency.

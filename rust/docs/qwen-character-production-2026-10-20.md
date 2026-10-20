# Qwen character release: 20 October 2026

The user explicitly approved deploying this useful starting point with the known
output defects recorded below. Activation receipts and final verification will
be appended after the managed worker and model server are switched.

## Release behavior

All six character routes (Journalist, Influencer, Scout, Insider, Analyst and
Oracle) use the same resident `Qwen3.5-4B-Q4_K_M` on localhost 8081, with thinking
disabled. Four server slots share one host concurrency budget of four. Each
request has a 7,900-token ceiling including reserved output; native tokenization
rejects oversized complete inputs rather than clipping source text. The model
server allocates 32,768 tokens across four slots, about 8k per slot.

Writing inputs omit routing URLs, gates and scores; retained source receipts keep
that metadata. Historical inputs retain complete headlines, publishers and dates.
Evidence scope, unresolved identity markers, negation and other semantic
qualifications remain in fresh input. Three obsolete 32,768-token option floors
are removed. All six prompts explicitly prohibit filling gaps and inventing facts.

The daily schedule, coverage controller, publication fences and parser guards
remain unchanged. No schema migration is required. API, Classifier and continuous
RSS acquisition on Archbox do not require a binary change for this character-only
release. The Mac character worker and its read-only eval tooling are rebuilt from
the release commit. The former Mac Smol server is stopped and disabled; its weights,
agent definition and recovery files are retained.

## Checks before activation

- 350 Rust library tests and the CLI checks passed; all-target compilation,
  formatting and diff checks passed.
- Current quality fixtures were recaptured from the same evidence and expected
  outcomes; historical benchmark receipts were preserved.
- Latest [Chelsea replay](../fixtures/character-runs/chelsea-2026-10-20-qwen-rerun/README.md):
  all 63 unique eligible headlines retained; four exact duplicates excluded.
  All seven Qwen requests fit below 7,900 tokens including output reservation;
  the largest budget was 7,112 tokens. No replay output was published.
- [Context study](../fixtures/character-runs/chelsea-2026-10-20-qwen-8k/README.md):
  the old 24-headline Qwen prompt used 10,568 tokens, including 8,535 tokens of
  routing metadata. Removing that metadata preserves the publisher evidence.
  A synthetic 58-headline/20-memory packet fits at 7,656 tokens including output.
- Credential-pattern scan: zero matches in approximately 2 MB of changed source,
  fixtures and retained replay artifacts; no oversized binary files included.
- Live preflight: daytime coverage phase, no running character claims, no long
  lock waiters. No open character edition has frozen Scout prompts to invalidate.

## Accepted starting-point limitations

The replay is not factual-prose qualification. Journalist can mix Berekum Chelsea
with Chelsea and merge unrelated player details. Insider can pass an unsupported
transfer denial despite its quote/co-mention guard. Scout can exhaust corrections
on sample wording; its guard needs review. Analyst can invent a performance reading
when Scout rejected. Influencer's positive reading is largely grounded in the
supplied Henderson headline, with interpretive wording still requiring review.

These findings remain saved with raw requests, replies and guard receipts. They
are explicitly accepted for this deployment; they must not be described as fixed.
Natural nightly completion and long-run throughput remain unqualified until observed.

## Recovery

Before activation, save the current worker launcher/environment, cognition agent,
Qwen agent and Smol agent/status in a private durable release directory. Install
matching committed binaries before changing the launcher. Stop the managed worker
gracefully, then reconfigure the managed model server and validate all four slots
before restarting the worker. Do not manually launch a second queue worker.

Rollback restores those saved files and service states, restarts the previous
model servers and boots the previous managed worker. There is no database migration
to reverse. Reverting prompts during a frozen character edition still requires
coordinating the edition's Scout input contract.

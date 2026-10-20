# Qwen character release: 20 October 2026

Deployed at **12:08 Eastern (16:08 UTC), 20 October 2026**, from runtime commit
`054105c60df16b5f823a6ec708d5f168dc44b759`. The user explicitly approved this
starting point with the known output defects recorded below. The exact runtime
commit is pushed to main and the active Archbox source checkout is synchronized.

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

## Activated state and verification

The managed Mac character worker restarted once, from the exact optimized release
binary. Its startup reports commit `054105c60df1`, six enabled character stages,
one model host, all six routes bound to Qwen on 8081, and `VOICE_NUM_CTX=7900`.
The same worker PID remained stable through postflight with no new startup errors.
The prior Smol service was unloaded and disabled, with its files retained.

The Qwen service now advertises four **8,192-token physical slots** sharing one
32,768-token pool. Logical requests are limited to **7,900 including output**.
The former `com.scoracle.qwen35-emotion` agent label is retained for compatibility;
it now serves every character. Thinking and context shifting are disabled, and
continuous batching is enabled. Native tokenizer and complete-response checks
remain active.

A concurrency smoke sent four retained Chelsea requests simultaneously. All four
slots were observed processing together, all four returned complete, untruncated
JSON with input-token counts matching the exact supplied token IDs, and no
request exceeded the logical budget. This is plumbing verification, not a factual
quality grade or a sustained throughput benchmark.

| Request | Input | Reserved output | Slot | Concurrent wall time |
| --- | ---: | ---: | ---: | ---: |
| Journalist | 5,012 | 2,100 | 0 | 38.221 s |
| Influencer | 586 | 600 | 2 | 24.972 s |
| Scout | 467 | 700 | 3 | 29.278 s |
| Insider | 1,533 | 1,400 | 1 | 32.554 s |

The deployed read-only eval tool additionally exercised four Oracle fixtures:
12/12 mechanical checks passed. Public API smoke passed 18/18; direct Archbox
API/database health was healthy. The API and Classifier services remained active.
The [exact runtime CI run](https://github.com/scoracle/scoracle-backend/actions/runs/39454056632)
completed successfully. No migrations or forced character publications occurred.
Postflight confirmed the existing midnight America/Detroit schedule still in
coverage phase, a current controller heartbeat, zero running character claims
and zero long lock waiters. A natural nightly edition remains unobserved.

An initial launchd bootstrap immediately after unloading Qwen returned error 5;
a later retry succeeded. The character worker remained stopped until four-slot
model verification passed. An initial Python public-health probe received HTTP
403; the repository's standard curl smoke subsequently passed all 18 public checks.
These diagnostics are retained rather than treated as successful first attempts.

## Exact release and recovery receipts

Private durable release directory:
`/Users/scotty/scoracle-worker/releases/qwen-all-characters-20261020-054105c6/`.
It contains `before/` launcher/environment/agent/service-state backups, the exact
binaries and hashes, `release.json`, `model-activation.json`, `activation.json`,
`verified.json`, worker startup and API/Oracle logs, and all four concurrent
native requests/responses with a slot trace. Configuration backups are private;
no credentials are committed.

| Artifact | SHA-256 |
| --- | --- |
| Mac character worker | `b45691456f035c877684ec427fdf59844293554cea89c9c04ddb836ecd198e18` |
| Read-only eval | `cdd66479fbcda63ccb3385e4ea2f4efe524653b0ea5ab8248046dd50bc9bcb7b` |
| Qwen3.5-4B Q4_K_M weights | `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4` |

The runtime changes affect the Mac character worker. Archbox's API/acquisition
binaries retain the previously deployed `49d27ca9` build; its source checkout is
updated, and those unaffected services were not restarted. No new database backup
or restore rehearsal was needed for this code/config-only release. Natural
nightly completion and broad semantic quality remain the next observations.

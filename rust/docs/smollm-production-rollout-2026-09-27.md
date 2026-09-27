# SmolLM3 production rollout — September 27

## Decision and release

The user selected Smol after the finite-palette probe recorded 21/21 valid compositions for both models and a 0.754-second Smol median versus 0.969 seconds for Granite on the same seven synthetic cases. Commit `17d43d11c385` sets the fallback Ollama model to `alibayram/smollm3`. It follows the Scout and remaining publisher palette commits (`00fc233e`, `d86bad6a`). The clean committed tree passed 545 Rust library tests with 60 ignored. The exact revision was pushed to `origin/main`.

## Live hosts

| Host | Live state | Evidence |
| --- | --- | --- |
| archbox | `17d43d11c385`, Smol on all nine model routes | User systemd worker active; startup journal confirms PostgreSQL, Ollama, nine Smol routes; local Ollama structured-output smoke returned `{"choices":[0]}`. The binary watcher was rearmed. |
| Mac mini | Previous `561815ebbb8f`, Granite on all explicit routes | Launchd worker active and connected to PostgreSQL after rollback. |

Archbox pulled `alibayram/smollm3` (the same 1.9 GB model blob as the Mac test), preserved its prior `.env.local` and worker binary as `.bak-20260926-smol`, and installed the new binary. A later service check found the worker and its binary watcher active with no model or palette errors in the preceding 20 minutes. This does not prove that every publisher processed a real item during that window.

## Mac blocker

The Mac new binary booted with Smol and the correct commit but received `No route to host (os error 65)` when connecting to PostgreSQL. The launcher resolved the current archbox address (`192.168.1.97`) and substituted it into the database target. A direct TCP check and `psql` query from the same Mac succeeded. The previous launchd worker binary connected immediately when restored. The new binary failed both in its new release directory and at the old executable path. This points to a macOS permission or code-identity boundary, but the exact OS decision has not been observed. The Mac is locked, so System Settings could not be inspected. The Mac route and executable were rolled back; the clean new binary remains staged under `~/scoracle-worker/releases/palette-smol-20260926/`.

Apple's [local network privacy technote](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy) says launchd agents need local network authorization and may fail before a prompt appears when they exit immediately. The Mac launcher is a user launchd agent. No global network privacy setting was weakened. An attempted manual launch of the full production worker was rejected by automatic approval review because it could run alongside the managed worker and process production queue items; that route was abandoned.

## Rollback

Archbox's previous binary is `rust/bin/scoracle-cognition.bak-20260926-smol`; its prior model configuration is `.env.local.bak-20260926-smol`. Restore both, pause/rearm `scoracle-cognition.path` around binary placement, and restart the user service if rollback is needed. Granite remains installed. On the Mac, `.env.local.bak-20260926-smol`, `run-worker.sh.bak-20260926-smol`, and `scoracle-cognition.bak-20260926-smol` preserve the prior state; the live files already match it.

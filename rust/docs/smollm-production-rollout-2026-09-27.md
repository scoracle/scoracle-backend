# SmolLM3 production rollout — September 27

## Decision and release

The user selected Smol after the finite-palette probe recorded 21/21 valid compositions for both models and a 0.754-second Smol median versus 0.969 seconds for Granite on the same seven synthetic cases. Commit `17d43d11c385` sets the fallback Ollama model to `alibayram/smollm3`. It follows the Scout and remaining publisher palette commits (`00fc233e`, `d86bad6a`). The clean committed tree passed 545 Rust library tests with 60 ignored. The exact revision was pushed to `origin/main`.

## Live hosts

| Host | Live state | Evidence |
| --- | --- | --- |
| archbox | `17d43d11c385`, Smol on all nine model routes | User systemd worker active; startup journal confirms PostgreSQL, Ollama, nine Smol routes; local Ollama structured-output smoke returned `{"choices":[0]}`. The binary watcher was rearmed. |
| Mac mini | Previous `561815ebbb8f` binary, Smol on all model routes, limited to `graph,editor,investigate_entity` | Launchd worker active, connected to PostgreSQL, and registered only those three handlers after the stage split. Its startup log confirms every configured route resolves to Smol. It cannot claim a publishing job. |

Archbox pulled `alibayram/smollm3` (the same 1.9 GB model blob as the Mac test), preserved its prior `.env.local` and worker binary as `.bak-20260926-smol`, and installed the new binary. A later service check found the worker and its binary watcher active with no model or palette errors in the preceding 20 minutes. This does not prove that every publisher processed a real item during that window.

A read-only production ledger check found no Smol-generated rows yet. The queue held 14 due momentum items and 15 due sigil items; every one was blocked by its declared upstream stage, so none was claimable. The other publisher rows were either scheduled for later or parked after prior failures. The live verification therefore covers routing, worker startup, database connectivity, model residency, and a direct structured-output call, but not a completed production publisher job.

## Mac blocker

The Mac new binary booted with Smol and the correct commit but received `No route to host (os error 65)` when connecting to PostgreSQL. The launcher resolved the current archbox address (`192.168.1.97`) and substituted it into the database target. A direct TCP check and `psql` query from the same Mac succeeded. The previous launchd worker binary connected immediately when restored. The new binary failed both in its new release directory and at the old executable path. Building the release again in the existing Mac checkout gave the same new ad hoc code-signing identifier, different from the prior worker's identifier. This points to a macOS permission or code-identity boundary, but the exact OS decision has not been observed. The Mac was locked, so System Settings could not be inspected. The Mac route and executable were rolled back; the clean new binary remains staged under `~/scoracle-worker/releases/palette-smol-20260926/` and is also built in the updated Mac worker checkout.

To complete the publisher model switch without waiting on the Mac UI, the Mac worker's `COGNITION_STAGES` was narrowed from all nine stages to `graph,editor,investigate_entity`, then restarted and verified. Its existing binary was then kept in place while `OLLAMA_MODEL` and all nine explicit route assignments were changed to `alibayram/smollm3`; another restart confirmed Smol on every route and PostgreSQL connectivity. Archbox's Smol worker remains registered for all nine stages, including every publisher. Thus all newly claimed publishing work uses the palette release and Smol, while the older Mac binary supplies only ingestion and evidence capacity with Smol. The Mac stage configuration is backed up as `.env.local.bak-20260927-stage-split`, and the immediately prior Granite configuration as `.env.local.bak-20260927-all-smol`. This routing is the live production state until the Mac binary permission issue is addressed.

Apple's [local network privacy technote](https://developer.apple.com/documentation/technotes/tn3179-understanding-local-network-privacy) says launchd agents need local network authorization and may fail before a prompt appears when they exit immediately. The Mac launcher is a user launchd agent. No global network privacy setting was weakened. An attempted manual launch of the full production worker was rejected by automatic approval review because it could run alongside the managed worker and process production queue items; that route was abandoned.

## Rollback

Archbox's previous binary is `rust/bin/scoracle-cognition.bak-20260926-smol`; its prior model configuration is `.env.local.bak-20260926-smol`. Restore both, pause/rearm `scoracle-cognition.path` around binary placement, and restart the user service if rollback is needed. Granite remains installed. On the Mac, `.env.local.bak-20260927-all-smol` restores the immediately prior Granite model assignments while keeping the three-stage split. `.env.local.bak-20260926-smol`, `run-worker.sh.bak-20260926-smol`, and `scoracle-cognition.bak-20260926-smol` preserve the earlier state.

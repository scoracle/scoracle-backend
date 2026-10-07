# Harness cleanup deployment — October 5, 2026

> **2026-10-06 governing update:** This dated record preserves its implementation and evaluation evidence. The [current contract](../../../scoracle-wiki/wiki/Architecture/Harness,%20Plugins,%20Tools,%20and%20LLM%20Contract.md) supersedes articulation-only ownership: cheap code owns collection; expensive compute owns discovery. Plugins assemble traceable clues through tools; the LLM reasons across them, discovers what they support and articulates the answer. Historical “plugin owns WHAT/model owns HOW” statements below do not govern new builds.

User authorized commit, push and deployment; context work is deferred to a separate session.

Runtime release: `0098df4ef07a9045b416d68bafc62d30956fca27` on `codex/harvester-cutover`, pushed to origin. The validated cleanup commit is `0098df4e`.

## Archbox — deployed

Fast-forwarded the clean production checkout and ran `scripts/hosting/release.sh`, building all four Go and three Rust production binaries before placement. API and cognition report `0098df4ef07a`. Both services are active with zero automatic restarts; rebuild watchers are active. The running cognition executable hash matches the installed binary (`439a04cdf6ef6d2eb0341b47b3c604dfa2f02a5bba0d5655ba8ee64235b66eb6`). Postgres connected and all ten existing plugins registered. Model routing and Harvester delivery enrollment are unchanged. Local and public API smoke checks each passed 18/18. No migrations or context changes were applied.

Release log and prior-binary backups: `/mnt/data/backup/scoracle/releases/harness-cleanup-0098df4ef07a/`. Services started at approximately 22:02 America/Detroit on October 5.

## Mac — staged, runtime update blocked

Fast-forwarded the worker checkout and staged the three Rust binaries built after the cleanup commit in `/Users/scotty/scoracle-worker/releases/harness-cleanup-0098df4ef07a/`. Preserved the previous executable, launcher and private environment in its `previous/` directory. Removed retired `editor` from the Mac stage list, retaining `graph,investigate_entity`.

The new managed worker reports the correct commit but fails to connect to Postgres with `No route to host (os error 65)`. Its launcher resolves and substitutes the current IPv4 address, and direct TCP connectivity to that address succeeds. System Settings shows three existing `scoracle-cognition` Local Network entries enabled; the new binary has a different ad hoc code-signing identity. This reproduces the [prior Mac blocker](smollm-production-rollout-2026-09-27.md); a code-identity/network-authorization boundary is suspected, not established.

Restored the previous executable and confirmed successful database connection, model reachability, two registered handlers and work-notification listening. The restored Mac binary was `561815ebbb8f`; its old packet-maintenance machinery remained active despite removal of the Editor stage. After explicit user approval, the Mac launchd worker was paused and its process stopped. The launcher still points to the previous executable. The new release is staged, not deployed successfully on Mac.

Automatic approval review rejected stopping the Mac worker and repointing the launcher to the failing staged binary, citing consequential production effects beyond the general deployment request. The user subsequently explicitly approved pausing the worker. `launchctl bootout` removed the Mac job, and process verification confirmed it stopped; its launcher was not repointed. Archbox remains the fully updated active fleet. The Mac runtime update needs resolution of its network failure before activation.

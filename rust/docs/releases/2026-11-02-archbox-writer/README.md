# Archbox shares Journalist generation — November 2, 2026

User requested moving some work to Archbox. Added one Journalist claim on Archbox alongside the existing Mac worker. No pipeline, prompt, model-weight or parser changes.

- Archbox: GTX 1070 Ti, 8 GiB VRAM. Existing d3 decision service remains active. Both models resident use approximately 5,915 MiB, leaving about 2.2 GiB.
- Copied the existing Mac Qwen3.5-4B-Q4_K_M model over the LAN. SHA-256 matched on both hosts: `00fe7986ff5f6b463e62455821146049db6f9313603938a70800d1fb69ef11a4`. No larger model/download.
- `scoracle-writer-model.service`: existing CUDA runtime, localhost port 8082, one slot, 10,240-token context, thinking disabled, no context shifting.
- `scoracle-writer.service`: existing verified `9a00d628ceab` cognition binary; only `narratives`, one in-flight claim, four database connections, controller disabled, 10,000-token requested window. ExecStart overrides the classifier-oriented shared environment without modifying it.
- Both new services are enabled at boot. The original classifier/controller, RSS collector, API and Mac writer remain running.
- The shared database claim fence still caps all hosts together at four active character jobs. Archbox takes one Journalist job; the Mac continues to handle Journalist, Insider and Influencer. Claims prevent duplicate concurrent ownership. The article cap remains ten.

## Verification

Ran the existing `character_outputs` fixture runner against the saved Dak Prescott case through production preparation and validation, with no database/publication: 1,146 native input tokens, 77 output tokens, 2.8 seconds, parser accepted. This is a compatibility smoke test, not an accuracy or speed comparison against the Mac.

Observed live Archbox Journalist calls, including a completed publication for NBA team 1 at 23:48:49 UTC. Sample live generation durations were 2.9–4.2 seconds. Source/name rejections also occurred and were retained by existing guards. All six Archbox services (API, classifier/controller, RSS, d3 model, Qwen model, writer) were active after activation.

Server-local fixture requests/responses and logs: `/mnt/data/backup/scoracle/archbox-writer-20261102/`. Production-derived local logs remain uncommitted. The three service/launcher files beside this README record the deployed configuration; they contain no credentials.

Rollback: `systemctl --user disable --now scoracle-writer.service`, wait for its current claim to drain, then `systemctl --user disable --now scoracle-writer-model.service`. The Mac continues consuming the same queue. No database rollback is needed.

Not yet measured: overnight aggregate throughput or performance while both inference services are busy. Existing factual-quality limitations remain.

# Rail Swap Closeout — the one-rail is the only rail

**Date:** 2026-08-15 · circled back 2026-08-16
**Status:** CLOSED. The swap plan is complete; the 08-16 circle-back ran the checklist,
found two real defects behind "my teams still show July", fixed both, and dropped the
last legacy table. See the 08-16 addendum at the bottom.

## What the swap turned out to be (Aug 14–15)

The trigger was "Chelsea hasn't updated since July." The diagnosis found three stacked
failures, and fixing them finished the rail swap end-to-end:

1. **The GPU lane was dead** — oMLX on the Mac had a 14B loitering beside the 8B and
   memory-guard-rejected every voice call for days. Fix: oMLX retired; the Mac runs
   Ollama serving a pinned `defiant-fable:9b` (num_ctx 4096 baked, keep_alive -1,
   MAX_LOADED_MODELS=1, parallel 6). The desktop Ollama app must stay quit.
2. **The D-T21 editor read cap collided across sports** — the count keyed on team id
   alone, the sweep runs NBA→NFL→FOOTBALL, so every Premier League club got zero reads
   from Aug 7. Fix: provenance carries `query_sport`; the cap counts the pair (50e2642).
3. **Nobody was watching** — cron green, daemon up, zero output, no alarm. Fix: the
   freshness watchdog (a55fb6b, sharpened 91875be), 08:30 + 20:30, reporting into
   `pipeline_runs`, non-zero exit on alarm.

Then the swap's long tail, all landed:

- **Topology**: prose voices (narrative/oracle/vibe/stats) on the Mac 9B; Insider +
  Analyst delegated to ministral-3:3b on the 1070 (~6× lane throughput; the parser
  fails-closed on the 3b's ~2% foreign-script leak and on defiant-fable's relabeled
  contract lines). Oracle A/B'd against the 3b and STAYS on the 9B — the 3b names the
  seats, pastes internal numbers, inflates scores (eval report, 2026-08-15).
- **ctx budgets** (journalist n20, vibe v19): mega-storylines had 11–12% of prompts
  silently over the 4096 window (max 17k tok). The news block and packet blocks are
  char-budgeted; cut evidence is NAMED. Post-deploy: zero over-window prompts.
- **Duty cycle**: 1h-on/1h-off, even hours ON, both machines rest (Scott: "both
  machines need an off block"). A daemon stopped on an odd hour is HEALTHY.
- **Old-rail demolition** (222): `narrative_episodes` + lifecycle + likelihood scorer
  + affiliation trigger + four dead tables gone; the memory cards repointed to
  storylines (prior story = resolved/dormant storyline; pair history = shared
  membership). Function signatures unchanged.
- **The seal repair** (223): `seal_storylines` had errored every night since mig 219
  (column alias), so nothing ever resolved AND the cron steps after it never ran.
  First run post-fix: 4 storylines resolved, 465 parts promoted. The /stories
  resolved archive is live on the site.
- **Frontend**: Stories page + 4-button tray deployed (scoracle.com/stories).
- **Run ledger**: the pipeline job records into `pipeline_runs` again (jobrun restored).

## The circle-back checklist (a few days out)

Ask these in order; each has a one-query answer:

1. **Watchdog green?** `SELECT * FROM pipeline_runs WHERE job='watchdog' ORDER BY id DESC LIMIT 6;`
   — expect success rows from 08:30/20:30. Any `failed` row carries the alarm text.
2. **Queue at steady state?** `SELECT status, count(*) FROM pipeline_work GROUP BY 1;`
   — pending should fall from ~5,700 toward a small working set over ~1–2 weeks
   (teams drained first via a one-time priority bump). If pending GROWS for days,
   inflow beats the duty-cycled drain — that's a capacity conversation.
3. **Ingest recording?** `SELECT * FROM pipeline_runs_latest;` — the pipeline row
   should date from last night, not Jun 28.
4. **Storylines resolving?** `SELECT status, count(*) FROM storylines GROUP BY 1;`
   — resolved should tick up as ground truth lands; the site's resolved archive grows.
5. **Prompts inside the window?** over_4k must stay 0:
   `SELECT count(*) FROM cognition_ledger WHERE built_prompt IS NOT NULL AND length(built_prompt) > 16384 AND generated_at > now() - interval '24 hours';`
6. **Big-club cards fresh?** Spot-check Chelsea/Man Utd/Villa news + momentum on the
   site — dates should be current-week, not July.
7. **Failure tail small?** `journalctl --user -u scoracle-cognition --since -24h | grep -c 'handler failed'`
   — single digits per day is the chaos tail; a spike means a new output shape
   (extend the parser the way 1b94dba did) or a sick lane.

## Deliberately open (small, non-blocking)

- `WATCHDOG_ALERT_URL` unset — alarms reach `pipeline_runs` + log, not a phone.
  One env line + an ntfy topic when Scott picks one.
- statcommentary doesn't write `pipeline_runs` (same class as the pipeline gap;
  its own log is healthy — 400/400 last night).
- The 3b-era `momentum: invalid response` one-offs retry themselves; not worth
  parser-chasing past the shapes already pinned as tests.
- Player-tail staleness until the rotation completes; the debounce keeps it honest
  after that.

## Where things live

- Watchdog: `scripts/hosting/cron-watchdog.sh` (cron 30 8,20)
- Duty cycle: `~/.config/systemd/user/scoracle-cognition-{pause,resume}.timer` (archbox,
  not in repo) — pause odd hours, resume even
- Routes + concurrency: archbox `.env.local` (`COGNITION_ROUTE_*`, backups
  `.env.local.bak-*` from each change)
- Mac model: `~/Library/LaunchAgents/com.scoracle.ollama.plist`; Modelfile pinned
  `defiant-fable:9b`
- Eval harness: `rust/target/release/eval` — the Oracle A/B invocation is in the
  usage header of `rust/src/bin/eval.rs`

## 08-16 circle-back — what the checklist actually found

Scott's report: "my teams all have data from the end of July." Chelsea's news/vibe
were dated Jul 25, sigil Jul 27 — while players shipped 999 news products in 48h.
Two distinct defects, both fixed same-day:

1. **The watchdog cried wolf** — `editor_reads[*] 0/N` was joining
   `news_article_readings`, the LEGACY rail's read ledger (frozen Aug 5). The one-rail
   Editor writes `editor_reads` and had read 1,539 articles in the prior 24h. Repointed;
   and the check now measures TEAM coverage (swept teams with ≥1 read, alarm <80%)
   instead of article share, which the D-T21 cap deliberately holds near 15% for
   high-volume sports (NFL sweeps ~70 articles/team, reads 10).
2. **Queue starvation inversion** — enqueue's ON CONFLICT restamped
   `available_at = NOW()` on still-pending rows. Claiming is FIFO on `available_at`, so
   every entity re-noticed with new input went to the BACK of the line. Hot teams get
   fresh articles nightly → re-stamped daily → never reached the head of a ~2,200-deep
   queue; quiet players aged to the front and took every slot. The entities Scott
   watches daily were structurally the least fresh. Fixes (e382a13): pending rows keep
   their FIFO place, and narratives/vibe/sigil claim teams before players (~200 bounded
   team rows — the player tail cannot starve in return).

Prune completed with it (8a3c790, mig 224): `news_article_readings` dropped;
`collapse_exact_title_duplicates` tiebreak repointed to `editor_reads`; the graph
junction's G1 legacy fallback removed; `bin/remap` (one-shot Jul backfill) and
`scripts/ops/article_read_drain_monitor.sh` deleted. Watchdog runs all-OK post-fix.

Checklist deltas for next circle-back: query 1's alarm text now reads
"N/M swept teams have a read"; the queue query (2) counts should show the player
backlog draining behind a permanently-fresh team set.

## 08-18 follow-up — the resolved archive was starving too

Two days of green watchdogs and current-week team cards confirmed the 08-16 fixes.
One new defect found chasing "storylines resolved stuck at 4": **every identity
adjudication since Aug 17 failed closed** ("invalid identity adjudication JSON").
The EmotionalNews seat now resolves to ministral-3:3b, which misses the bare
`json_mode` shape 4/5 times — so nothing reached `transfer_identity_applications`
(applied), the `transfer_ground_truth` view stayed empty (newest row Jul 29), and
`seal_storylines` resolved 0 nightly with nothing to seal against. Fix (6850b2c):
the adjudication contract became a format schema (the Editor's own D-T43 pattern on
the same model), and fail-closed rows now store the model's verbatim reply instead
of "" — the Aug-17 failures were undiagnosable from the database.

Also re-frozen: the 12 editor eval fixtures the friction-audit ep7 bump left at ep6
(bf6bc4b, regenerated through the real builder; diff = the contract delta only).

**The one open watch-item is capacity**: pending grew 7,012 → 8,874 in two days
(player products, ~800/day net). Teams stay permanently fresh by construction, so
the product face is unaffected — but the player tail lags further behind each day.
The levers, when Scott wants one: more on-hours in the duty cycle, higher parallel
on the 3b lane, or trimming player product inflow (only enqueue players the Editor
actually placed in a storyline that cycle).

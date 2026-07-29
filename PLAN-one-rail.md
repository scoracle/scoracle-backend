# PLAN — One Rail

**STATE: Phase 0 COMPLETE (ground truth verified, no STOP). Next: Phase 1 (substrate, migrations
200+). Last plan commit: (this one). Updated 2026-07-29.**
*(Phase 0 deltas that bind later phases: §0.8 rewritten — psql runs on **archbox** over ssh, not on
the Mac; and the **archbox checkout is behind this repo** (`cec766a`), with migrations 198/199
untracked there — sync it before Phase 1 runs `sql/migrate.sh`. D-1 answered: `mistral-nemo:12b`.)*
*(Executors: keep this line current — phase pointer, last commit hash, date — every commit.)*
*(Revised 2026-07-29, pre-execution audit: OLMo removed — measured, it does not hold the 4
slots on the 1070 Ti; front page deferred to Appendix B D-6; teams.kind migration deferred
into D-3; duplicate short-circuit added in 3.4. Capacity corrected in 3.9 — the legacy
~7,400 reads/day was DEMAND-limited under the 2h-on/1h-off rest schedule with the card mostly
idle, not a ceiling.)*

Written 2026-07-28. This is the build order for the greenfield rail decided in
[`HANDOFF-newsroom.md`](HANDOFF-newsroom.md). Read that file's §1–§3 before touching anything —
it is the case for this plan and the map of the rot this plan must not rebuild.
[`PLAN-ingest-simplification.md`](PLAN-ingest-simplification.md) stays the reference for the
**traps (T1–T13)** and the measurements; its build order is dead.

The two rails — news and stats — collide into one:

```
LUNGS   Google News RSS, one ranked query per team, daily        the query IS the hypothesis
   |    (Go: fetch + store + body retention. Zero judgment.)
   v
HEART   THE EDITOR reads the body (archbox 1070 Ti, 4 slots)
   |      gate / discover / type / register  ->  editor_reads
   |      storylines assembled in code       ->  packets
   |    THE INVESTIGATOR verifies (same card, same slots)
   |      unresolved name -> web evidence -> persons/aliases/facts   (living database)
   |      result line     -> box-score scrape -> event_box_scores    (stats rail reborn)
   v
BRAIN   the voices (Mac, Mistral 12B, 4096 ctx, concurrent)
          Journalist · Influencer · Insider  (packet subscribers, by tag)
          Scout  (confirmed facts + the stats platform only — never prose)
          Analyst (peer-aware) · Oracle (reads the five cards — unchanged)
```

**Naming ruling (Scott, 2026-07-28): the acquisition character is THE INVESTIGATOR.** Older docs
(wiki Living Database, the 2026-07-27 planning docs) say "the Seeker" — same character, old name.
Code, tables, and stages use `investigator`/`investigate_*`. Wiki updates land in Phase 8, not
before.

---

## §0 — How to work this plan

This plan is executed one phase per session, by smaller models, on mobile. The protocol:

1. **Read first:** this section, the **STATE** line, the phase you are executing, and any appendix
   that phase names. Do not read the whole repo. `HANDOFF-newsroom.md` §3 and the Traps section of
   `PLAN-ingest-simplification.md` are the background; consult them when a step cites T-numbers.
2. **Work the phase top to bottom.** Checkboxes are ordered by dependency. Do not reorder, do not
   start the next phase.
3. **STOP on surprise.** If a Verify step returns a number outside its stated band, or a named
   file/function/table does not match what the step claims: stop, write what you found in the
   phase's Log, commit, and emit the resume block with a `BLOCKED:` line prepended. Do not
   improvise a fix. Only measurement has ever settled anything here — do not ship a theory.
4. **One change, one measurement** (ar4/ar5 lesson). Never bundle two behavior changes in a step.
5. **Migrations:** follow `sql/migration_template.sql` (BEGIN; DDL; self-record into
   `schema_migrations`; COMMIT). Apply with `sql/migrate.sh`. Then run
   `scripts/hosting/snapshot-schema.sh` and **commit the migration + snapshot together**. Current
   highest migration is **199**; number from 200 up (176 is a known gap — leave it).
6. **Deploys are explicit.** Building into `go/bin/` or `rust/bin/` trips the `.path` watchers and
   restarts services, overriding the rest windows (harness pauses 00/03/06/09/12/15/18/21:00 +1h).
   For tests and rehearsals build to `target/debug/`. Only steps marked **[DEPLOY]** place
   binaries. Services are user systemd units on archbox (`systemctl --user`).
7. **Data writes are rehearsed** in a rolled-back transaction first, with the invariant asserted
   inside the transaction (the `remap -rollback` habit). Any write that touches
   `news_article_entities.vetted` must suppress triggers with
   `SET LOCAL session_replication_role = 'replica'` (T10) — and never
   `ALTER TABLE ... DISABLE TRIGGER` (ACCESS EXCLUSIVE against a live pipeline).
8. **DB access — from archbox, not the Mac** (corrected by Phase 0). Postgres 18.4 (`scoracle`,
   datadir `/mnt/data/postgres/data`) runs on archbox; the Mac has **no `psql` installed** and its
   `.env` carries **empty** `DATABASE_URL`/`DATABASE_PRIVATE_URL` (no `.env.local` on the Mac — only
   a `.bak`). The working incantation from a Mac session is:
   ```
   ssh archbox 'cd ~/scoracle/scoracle-backend && set -a; . ./.env; . ./.env.local; set +a; \
     psql "${DATABASE_PRIVATE_URL:-$DATABASE_URL}" -c "select 1"'
   ```
   `~/.ssh/config` already defines `archbox` (192.168.1.92, user `sheneveld`, key auth — BatchMode
   works). Prefer `DATABASE_PRIVATE_URL`, as `sql/migrate.sh` does.
9. **Finish ritual, every session:** tick the checkboxes you completed, fill the phase Log with the
   numbers you measured, update the STATE line, commit everything (plan file included) as
   `rail: phase <N> — <short description>`, then print the next phase's **resume block** from the
   phase's Handoff fence, verbatim, as the last thing in your reply so it can be copied with one
   tap.
10. Long-poll background watchers get killed in this environment — use point-in-time checks.

**The non-negotiable design law (T2): describe, then derive.** A local model never renders a
verdict, a routing decision, or an identity match as a bare field. The model describes what the
text/page says; code computes the judgment. Every contract in this plan is shaped by this.

---

## §1 — The packet (the contract; everything else is knobs)

Per HANDOFF §7, the contract comes before any schema. The **packet is the product**: one
storyline, multi-tagged, entity-indexed, append-only.

### 1a. `editor_reads` — one row per article, the Editor's read (contract `ep1`)

The greenfield successor to `news_article_readings` (which the legacy `article_read` stage keeps
writing until cutover; two writers must never share one table, so this is a new table).

| column | meaning |
|---|---|
| `article_id bigint PK` → news_articles | |
| `status text` | same taxonomy as `news_article_readings.status` (success/irrelevant/paywall/blocked/empty_body/fetch_failed/parse_failed/duplicate + `not_sport`) |
| `contract_version text` | `'ep1'` — a cache key, not a label (T1) |
| `model_version, parser_outcome, last_error` | as in legacy |
| `final_url, final_domain, content_hash, extracted_words` | fetch provenance |
| `read jsonb` | the full ep1 model envelope (below) |
| `resolved jsonb` | resolver outcome, written by code: `{links:[{entity_type,entity_id,sport,via_surface}], unresolved:[{name,kind_hint,descriptor}], refused_ambiguous:[...]}` |
| `storyline_id bigint NULL` | attach result (Phase 4 writes it) |
| `fetched_at, updated_at` | |

**The ep1 model contract** (constrained decoding; property order IS the contract — extraction
before anything derived, the ar4 lesson). Fields, in schema order:

1. `source_language`
2. `page_kind` — `article|score_table|listing_or_schedule|video_clip|roundup|other`
3. `names[]` max 12 — **the discovery channel.** Every person and club the TEXT involves:
   `{name, kind_hint: person|club|national_team|other, descriptor}` where `descriptor` is ≤6 words
   **from the text** naming role/club/context ("Real Madrid manager", "PSG sporting director",
   "city hosting the finale"). This merges ar7's `relevant_entities` with F7's describe-facts: the
   descriptor is what lets code refuse `Paris`-the-city → Paris-the-club and route
   `kyle shanahan` → person-candidate instead of a fuzzy player match (T9).
4. `entity_roles[]` — `{entity, role: subject|opponent|passing_mention|absent}` over the provided
   hypothesis list (the query entity + any pre-linked entities). Unchanged from ar7.
5. `story_type` — `transfer|injury|performance|fixture|roster|contract|general` (ar7 enum, kept
   for fixture continuity; hirings are `roster`).
6. `result_line` — the **verbatim** final-score line if the text states a completed result
   (`"Real Madrid 2-1 Arsenal"`), else empty string. Verbatim-or-empty is the describe-then-derive
   shape: code parses it; the model never says "a game happened."
7. `register_phrase` then 8. `register` — phrase BEFORE label (describe → label), enum
   `celebration|outrage|resignation|anticipation|neutral`. The Influencer owns the number; the
   Editor never scores.
9. `key_facts[]` max 8 — one claim per fact, attributable ("The Athletic: deal not agreed").
10. `caveats`, 11. `evidence_blurb`

**Dropped from ar7:** `co_mentions[]` — the headline-candidate voting loop is the redundancy
HANDOFF §1 names; the numbered-candidate mechanism dies with the rail. Derivations (all in code,
none asked of the model): relevance; entity links (exact `nrm()` match on
`entity_name_surfaces` only — trigram ranks for review, never writes, T9); Investigator
nominations (unresolved `names[]`); routing tags (from `story_type` + non-neutral `register` →
tag `charged`); box-score nomination (`result_line` parses + `page_kind` ∈ article|score_table);
mention quotes (code slices ±160 chars around the name's first occurrence in the **stored**
`full_text` — the model never emits quotes; bodies are retained now precisely so evidence can be
sliced deterministically).

### 1b. Storylines — assembled in code, never compiled by a model

| table | columns |
|---|---|
| `storylines` | `id bigserial PK, sport, title text` (display only — first member's headline; never used for matching), `status open|resolved|dormant`, `first_seen_at, last_seen_at, resolved_at, resolution jsonb` |
| `storyline_articles` | `storyline_id, article_id` PK pair, `attached_at`, `attach_method auto|backfill` |
| `storyline_entities` | `storyline_id, entity_type, entity_id, sport, role, joined_at, last_seen_at, left_at, exit_reason` — D5: the entity's part has its own lifespan; when a storyline resolves, name who it resolved for and close every other edge as `not_the_outcome` in one stroke, in code |

**Attachment rule (deterministic, logged, no model call):** candidate set = open storylines in the
article's sport sharing ≥1 resolved entity, `last_seen_at` within 14d. Score = entity overlap
count + 1 if `story_type` matches the storyline's dominant type + recency bonus. Attach to the top
scorer above threshold 2; else open a new storyline. Free-text story names are banned from
matching (D3's lesson) — entities + type + time ARE the join key. BGE/cosine is not rebuilt (F2).

### 1c. `packets` — the compiled brief, append-only

A packet is a **snapshot of a storyline at compile time**. Compiled **in code** from member
`editor_reads` (zero model tokens — C5: the Editor's output budget is coverage). A new packet
supersedes the prior one; packets are never edited (archive-as-moat).

| column | meaning |
|---|---|
| `id bigserial PK, storyline_id, day date, sport, compiled_at` | |
| `headline text` | best member title (lowest `feed_rank`) |
| `story_types text[], register, register_phrase` | rollups (register = strongest non-neutral among members, with its phrase) |
| `claims jsonb` | `[{article_id, source, fact, published_at}]` — **contested state is preserved**: "agreement reached" and "yet to reach agreement" both stay, attributed (T3/D6 — the disagreement IS the story; 0.5–0.75 similarity attaches, never collapses) |
| `facts jsonb` | thin, structured only: `{story_types, result_line?, entities}` — the Scout never reads packet prose (T4); confirmed facts reach it by other roads (§4) |
| `quotes jsonb` | code-sliced from stored bodies |
| `routing_tags text[]` | derived: story types + `charged` |
| `slice_fingerprints jsonb` | per-voice hash of that voice's slice (E2): `{journalist: h, vibe: h, transfers: h}` — a packet re-fans only to voices whose slice moved |
| `unresolved_mentions jsonb` | B3's census, rolled up onto the packet |
| `supersedes_packet_id bigint NULL, contract_version 'pk1'` | |

*(A ranked "front page of the day" model call was audited out pre-execution: the packets ARE the
compiled stories of the day, and a ranking product with no client surface is decoration.
Appendix B D-6 holds the half-day of work for the day a surface exists.)*

---

## §2 — The cutover (defined now, measured later; HANDOFF §7)

**The single condition.** The Journalist reads packets instead of
`load_vetted_corpus_with_exclusions` (`rust/src/junctions/journalist/mod.rs:380`) when, for **7
consecutive days**, all five clauses hold (SQL for each lives in Phase 7):

1. **Coverage:** ≥95% of articles ingested each day have an `editor_reads` row within 24h.
2. **Packet presence:** every (entity, day) that legacy produced a narratives corpus for (≥3
   vetted canonical articles) appears in ≥1 packet's `storyline_entities` that day.
3. **Precision:** a daily 50-link sample from `editor_reads.resolved.links` audits ≥95% correct
   (the B4 flip standard; the legacy rail measured ~95% on flips, ~75% on brand-new).
4. **Gates green:** `eval --task editor --fixtures` passes 100%, and Editor/Investigator
   dead-letter count (attempts ≥ 5) is 0 over the window.
5. **Accounting:** every packet's claims reference member articles only; ledger reconciliation
   finds 0 unaccounted drops (the A5 rule: an article dropped from evidence must be named).

**The flip is one act:** `RAIL=packet` in both machines' env + the Phase 7 [DEPLOY]. Scott flips
it; the harness never auto-promotes.

**What happens to the old rail that day: it stops.** Same session, in order: legacy triggers
dropped, Go stops enqueueing `scrub`, `COGNITION_STAGES` drops `article_read` + `scrub`. Deleted
— not left running in parallel forever. Source excision follows in Phase 8 (Appendix A is the
inventory); rollback stays possible for 7 days (env flip back + revert migration in Appendix A).

**The old corpus is archive.** 150,566 articles, 265,204 links, and every `news_article_readings`
row keep their state forever. No backfill of packets over history. The new rail is forward-only
from flip day.

---

## §3 — Topology

| organ | host | model | concurrency | notes |
|---|---|---|---|---|
| Lungs | archbox (Go) | none | — | Google News RSS, teams-only sweep, daily 02:00 cron. The query is the hypothesis; Go decides nothing. |
| Heart: the Editor (module `rust/src/junctions/editor/` — greenfield; stage `editor`) | archbox GTX 1070 Ti | `gemma3:4b` (the engine — §4 ruling; OLMo is out) | shares the 4-slot group | `ARCHBOX_GEMMA_SLOTS` (`rust/src/stage.rs:84`) is the pool. The legacy seat is renamed `junctions/article_reader/` (Phase 3.0) and dies in Phase 8. |
| Heart: the Investigator (module `rust/src/junctions/investigator/`; stages `investigate_entity`, `fixture_boxscore`) | archbox, same card | `gemma3:4b` (same resident model — no VRAM swap thrash) | same 4-slot group | Scott's call: the Investigator rides the Editor's card |
| Brain: 6 voices | Mac | Mistral 12B (exact Ollama tag: Appendix B; routes are config, not doctrine) | 3 concurrent to start | `num_ctx` 4096 — the packet renderer's hard budget exists because of this |

The plumbing for all of this **already exists** (verified 2026-07-28): per-role
`COGNITION_ROUTE_<ROLE>_BASE_URL` (`rust/src/config.rs:269`), per-host semaphores via
`COGNITION_BACKEND_CONCURRENCY` `url=permits` (`config.rs:301`, `route.rs:315 governor_for`),
per-call `num_ctx` (`ollama.rs:44`), slot groups (`stage.rs:77`). New roles: `Role::Editor`
(`COGNITION_ROUTE_EDITOR`) and `Role::Investigator` (`COGNITION_ROUTE_INVESTIGATOR`); legacy
`Role::ArticleReader` dies in Phase 8.

Two worker deployments drain one Postgres queue; `COGNITION_STAGES` on each machine decides who
claims what: archbox = editor/investigate_entity/fixture_boxscore (+ legacy stages until cutover),
Mac = the six voice stages.

---

## §4 — Rulings carried into this plan (do not re-litigate)

- **Files are named for characters** (Scott, 2026-07-28). Junction modules carry the character's
  name — `editor/`, `investigator/`, `journalist/`, `insider/`, `influencer/`, `scout/`,
  `analyst/`, `oracle/` — because the wiki, the contracts, and the cast all speak that language.
  Queue stage strings keep product names (`narratives`, `vibe`, `peak`, …; the new `editor` and
  `investigate_entity` stages happen to align). **Renaming a live identifier is still forbidden**
  (T1/F1): stage strings, env keys, prompt versions, and table names never change as part of a
  file move. Files are safe to rename; identities are not.
- **Gemma 3 4B is the Editor's engine — settled by hardware.** OLMo was tried and does not hold
  the 4-slot concurrency on the 1070 Ti (Scott, 2026-07-29). Concurrency IS coverage, so holding
  4 slots on the card is a precondition any future challenger must meet before quality is even
  measured (`COGNITION_ROUTE_EDITOR_CANDIDATE` exists for that day). No bakeoff is scheduled.
- **The regex tier goes entirely** — not demoted to clerk. The Google query is the only
  hypothesis Go contributes, recorded as the primary link (`match_confidence 0.95`,
  `go/internal/thirdparty/news.go:350`). HANDOFF §1.
- **Known consequence of that deletion** (recon 2026-07-28): the regex secondary pass
  (`news.go:363-392`) is today the sole producer of player↔article links and `title_pos`. After
  cutover the Editor's resolver writes player links; `title_pos` stays NULL on new links, which
  the co-mention/heat proximity gates treat as lenient. Co-mention refresh dies at demolition;
  `compute_transfer_heat` survives on Editor links — its volume gets a T7-style rebaseline in
  Phase 8.
- **`vetted` becomes one fact: the Editor linked it.** Scrub-as-judge dies. The two-writer
  tri-state dies.
- **Exact match on `nrm()` surfaces is the only automatic link path** (T9). Trigram ranks for
  review. Ambiguity is refused, recorded, and nominated to the Investigator.
- **The Editor nominates; the Investigator verifies; search discovers; sources prove.** A model
  mention is never a database write. Every accepted fact cites a `source_documents` row.
  (Living-database doctrine, planning doc 2026-07-27.)
- **Maintenance is demand-led, like growth.** The story that makes a fact stale is the same story
  that re-arms its verification: a new mention of a decided candidate reopens it (5A.6),
  re-verification supersedes changed relationships with dated validity, box scores revive
  `player_team_history` and tiers through the existing triggers, and the transfer chain keeps
  adjudicating movement. Nothing polls the world on a schedule to stay fresh; relevance drives
  refresh.
- **No prose reaches the Scout** (T4). The Scout's inputs stay: the stats platform (now fed by
  scraped box scores) + threshold-gated confirmations (`transfer_identity_applications` pattern).
- **Voices read packets; packets are compiled in code.** The Editor's model budget goes to
  reading articles (coverage), not to summarizing summaries.
- **`momentum_scores` (deterministic, ±100 scale) ≠ `momentum_summaries` (voice output, −5..+5).**
  Conflating them caused the 715-WARN storm. Decided facts are computed in code; models narrate.
- **ar7/C2 must never deploy onto the legacy rail** (HANDOFF §5). Its contract IS ep1's ancestor;
  it ships as the greenfield `editor` stage, not as an `article_read` bump.

---

# The phases

## Phase 0 — Ground truth (read-only; no writes, no deploys)

Purpose: verify this plan's recon against the live system before any DDL is authored, and prove
the executor toolchain works from a fresh session.

- [x] **0.1** `set -a; source .env; set +a; psql "$DATABASE_URL" -c "select 1"` works. If the
      variable is named differently, fix §0.8 and note it in the Log.
      → **DELTA (§0.8 rewritten):** not the variable name — the *host*. No `psql` on the Mac; Mac
      `.env` has empty DB URLs. Works from archbox over ssh. See Log A.
- [x] **0.2** Confirm migration state: `select max(version) from schema_migrations` → expect
      `199_refresh_surfaces_analyze` band; `ls sql/migrations | tail -3` matches. Confirm
      `sql/schema/schema.sql` snapshot is current (no unapplied files).
- [x] **0.3** Confirm the queue substrate: `\d pipeline_work` shows `stage text` with **no** CHECK
      on stage; `entity_type` CHECK is `player|team|article|fixture`; PK
      `(stage, entity_type, entity_id, sport)`.
- [x] **0.4** Confirm `news_article_entities.entity_type` CHECK is `('player','team')` and
      `entity_name_surfaces` CHECK likewise; count rows in `entity_name_surfaces` (expect ~16,700).
- [x] **0.5** Confirm `news_articles.full_text` is NULL for all rows
      (`select count(*) from news_articles where full_text is not null` → 0).
- [x] **0.6** Confirm live triggers by name: `enqueue_derive_on_vetted`,
      `enqueue_transfers_if_transfer_related`, `enqueue_voices_on_routing_tags` on the news
      tables; `stage_routing_subscriptions` is empty.
- [x] **0.7** Confirm stage wire names in `rust/src/work.rs:42-55` = `scrub, article_read,
      fixture_boxscore, graph, peak, momentum, transfers, narratives, vibe, sigil`; and
      `ARCHBOX_GEMMA_SLOTS = ("archbox-gemma3", 4)` at `rust/src/stage.rs:84`.
- [x] **0.8** Confirm per-role base-url plumbing exists: `COGNITION_ROUTE_<X>_BASE_URL` in
      `rust/src/config.rs` (~:269) and `COGNITION_BACKEND_CONCURRENCY` (~:301).
- [x] **0.9** Confirm `fixture_boxscore_fetches` exists (mig 189) and
      `rust/src/boxscore_fetch.rs` fetches **provider JSON** (BallDontLie pagination const) — the
      landing table is reusable, the sources are dead (subscriptions cancelled 2026-07-27/28).
- [x] **0.10** Record baseline numbers in the Log: articles/day by sport (7-day avg), current
      `article_read` queue depth, `news_article_readings` rows, `narrative_persons` count,
      current disk free on the DB volume (bodies will add ~40–80 MB/day).
- [x] **0.11** Confirm the Mac's Ollama is reachable from archbox and note the exact installed
      Mistral 12B tag (`curl http://<mac>:11434/api/tags`). Record hostname/IP in the Log. If the
      Mac is unreachable, note it — Phase 6 is the first phase that needs it.

**Verify:** every box above either matched or has a Log entry explaining the delta and a plan
edit. **Commit:** `rail: phase 0 — ground truth verified`.

### Log (phase 0)

Executed 2026-07-29 (session start 06:40 EDT), read-only. **No STOP condition hit** — every named
file, function, table, trigger and constant matched the recon; every number landed in its stated
band. Two deltas, both about *how the executor reaches the machines*, not about what the plan
claims: **A** (DB access host — §0.8 rewritten) and **B** (archbox checkout is behind — a Phase 1
precondition). Recorded below.

**A. DB access is via archbox, not the Mac (0.1 — §0.8 rewritten).** The Mac has no `psql` binary
(no libpq, no Postgres.app) and its `.env` ships `DATABASE_URL=` / `DATABASE_PRIVATE_URL=` **empty**;
`.env.local` does not exist on the Mac (only `.env.local.bak.20260726-111052`). The live DB is
**PostgreSQL 18.4**, database `scoracle`, datadir `/mnt/data/postgres/data`, on archbox
(`192.168.1.92`, user `sheneveld`, already in `~/.ssh/config`, key auth works under `BatchMode`).
Credentials live in `~/scoracle/scoracle-backend/.env.local` **on archbox** (17,564 B, 2026-07-27).
§0.8 now carries the working incantation. Every measurement below was taken through it.

**B. The archbox checkout is behind this repo — Phase 1 must sync it first.** archbox
`~/scoracle/scoracle-backend` is on `main` at **`cec766a`** ("Schema snapshot: 195/196/197"), while
this repo is at `90582f3`. On archbox, migrations **198 and 199 are untracked files** (applied to
the DB, never committed there) and `sql/schema/schema.sql` + `schema_migrations.txt` are
**modified-uncommitted**. The DB itself is fully correct (0.2 below); only the checkout drifted.
Phase 1 authors migrations 200+ in *this* repo but must apply them from archbox — **sync archbox to
this HEAD (or copy the migration file over) before running `sql/migrate.sh`**, or the runner will
also sweep up 198/199's stale working-tree state into the snapshot. Not a blocker; a first step.

**0.2 — migrations ✅.** `max(version) = 199_refresh_surfaces_analyze` (applied 2026-07-28
15:28:41 EDT), exactly the expected band. **201 files / 201 recorded, zero drift in both
directions** (`comm` both ways empty) — nothing unapplied. The **176 gap is confirmed real**
(174, 175, 177, 178 present; no 176) — leave it; number from 200 up. `sql/schema/schema.sql`
(537,374 B) was regenerated 15:29, one minute after 199 landed, and is committed clean
(`git status sql/` empty here). Snapshot is current.

**0.3 — queue substrate ✅ exact.** `pipeline_work`: `stage text NOT NULL` with **no CHECK on
stage** (so a new `editor` / `investigate_entity` stage needs no DDL — as the plan assumes);
`entity_type` CHECK = `player|team|article|fixture`; PK `(stage, entity_type, entity_id, sport)`.
Also present, unclaimed by the recon but worth knowing: `status` CHECK `pending|running|failed`,
partial claim index `(stage, available_at) WHERE status IN (pending, failed)`, and statement-level
`notify_pipeline_work_ready()` triggers on INSERT/UPDATE (LISTEN/NOTIFY wakeup, not polling).

**0.4 — entity CHECKs ✅.** `news_article_entities_entity_type_check` = `('player','team')`;
`entity_name_surfaces_entity_type_check` = `('team','player')` (same set). Bonus:
`entity_name_surfaces_surface_kind_check` = `('name','alias')`.
**`entity_name_surfaces` = 16,690 rows** — in band (recon said ~16,700).

**0.5 — bodies ✅.** `select count(*) from news_articles where full_text is not null` → **0**, out
of **159,601** total articles. Body retention is genuinely greenfield; nothing to migrate.

**0.6 — triggers ✅ all three, by exact name, all enabled (`tgenabled='O'`).**
`enqueue_derive_on_vetted` — AFTER UPDATE OF `vetted` ON `news_article_entities`, FOR EACH ROW,
WHEN `new.vetted IS TRUE AND old.vetted IS DISTINCT FROM new.vetted`.
`enqueue_transfers_if_transfer_related` — AFTER UPDATE OF `bucket` ON `news_articles`,
WHEN `new.bucket = 'transfer'`. `enqueue_voices_on_routing_tags` — AFTER UPDATE OF `routing_tags`
ON `news_articles`, WHEN `routing_tags IS DISTINCT FROM`. These are the three the T10 rule is
about. **`stage_routing_subscriptions` = 0 rows** ✅ (empty, as claimed).

**0.7 — stage wire names ✅ exact.** `rust/src/work.rs:44-53` is verbatim `scrub, article_read,
fixture_boxscore, graph, peak, momentum, transfers, narratives, vibe, sigil` (10 stages; the
`as_str` match spans 42–55 as the recon said). `rust/src/stage.rs:84`:
`pub const ARCHBOX_GEMMA_SLOTS: (&str, usize) = ("archbox-gemma3", 4);` ✅ — and its doc comment
already reads "shared by The Editor and graph", so the slot-group seat the Editor takes is real
and documented today.

**0.8 — per-role plumbing ✅, line refs dead on.** `COGNITION_ROUTE_<ROLE>_BASE_URL` resolved at
`config.rs:269-271` (via `normalize_base_url`, trailing slash trimmed so one host is one backend);
role key built at `:265`; `COGNITION_BACKEND_CONCURRENCY` parsed at `:301-304`, parser at `:316`;
`governor_for` at `route.rs:312-315`; per-call `num_ctx` at `ollama.rs:44` (applied `:153-154`,
omitted when `<= 0`). **`COGNITION_ROUTE_<ROLE>_CANDIDATE[_BASE_URL]` also exists** (`config.rs:281-295`),
defaulting a challenger to its role's own host — the §4 `COGNITION_ROUTE_EDITOR_CANDIDATE` hook is
already built. Nothing new is needed to put the Editor on archbox and the voices on the Mac.

**0.9 — box-score landing table ✅.** `fixture_boxscore_fetches` exists (mig
`189_fixture_boxscore_fetches.sql`), PK `fixture_id`, FKs to `fixtures`/`sports`, payload columns
`score / period_scoring / team_stats / player_stats / raw_labels` (jsonb) **plus
`model_version, prompt_version, parser_version, output_contract_version, parser_outcome`** — the
table was already shaped for a model-parsed path, which is exactly what 5B needs; it is reusable
as claimed. Status CHECK already includes `not_supported|not_final|not_found|blocked|fetch_failed|
parse_failed|validation_failed`. **1 row** in it (the dead-provider era). `rust/src/boxscore_fetch.rs`
(57,785 B) fetches **provider JSON** from `https://api.balldontlie.io` with cursor pagination and
`const MAX_BDL_PAGES: usize = 20` (`:26`), `per_page=100`, key from `BALLDONTLIE_API_KEY` — dead
sources, live landing table. Confirmed.

**0.10 — baselines.**

| baseline | value |
|---|---|
| Articles ingested/day, steady state (07-25 → 07-28, 4 full days) | **5,584/day** (22,337 total) |
| — FOOTBALL | 3,467/day |
| — NFL | 1,365/day |
| — NBA | 763/day |
| `article_read` queue depth at 06:40 EDT | **7,426 pending**, 2 failed |
| `news_article_readings` rows | **20,970** |
| `narrative_persons` | **2,405** |
| `entity_name_surfaces` | 16,690 |
| `news_articles` total | 159,601 |
| DB size | **11 GB** |
| Disk free on DB volume (`/mnt/data`, 1.9 T) | **1.8 T free, 2% used** |

Notes that change how these read:
- **Ingest has no `sport` or `created_at` column.** `news_articles` is keyed by `fetched_at`, and
  sport arrives only via the `news_article_entities` join. Per-sport rates above are
  `count(distinct article)` through that join, so an article linked to two sports counts in both;
  the 5,584/day total is the un-joined truth.
- **2026-07-24 was a 32,160-article backfill spike** — a naive 7-day average reads 9,286/day and is
  wrong. The 4-day window above excludes it. (07-22: 871, 07-23: 1,360, 07-24: **32,160**,
  07-25: 5,117, 07-26: 2,818, 07-27: 6,046, 07-28: 8,356, 07-29 partial: 9,035.)
- **Body retention is a non-issue on disk.** At the plan's 40–80 MB/day, 1.8 TB free is ~60+ years.
  The DB volume is a separate 2 TB NVMe; note the **root volume `/` is at 86% (15 G free)** — the
  DB is not on it, but archbox's root is the tighter resource.
- **The 7,426 depth is a rest-window reading, not a backlog.** Sampled 38 min into the scheduled
  pause. `logs/queue-depth.csv` (10-min sampler, `scoracle-qsample.timer`, has a `harness_active`
  column) shows `article_read` pending per day as **min → max → end-of-day**: 07-27 `70 → 6,095 → 70`;
  07-28 `1 → 8,133 → 20`; 07-29 (through 06:40) `4,197 → 8,754 → 7,426`. **The legacy rail drains
  its day to ~zero every day.** This is direct evidence for the pre-execution audit's correction:
  ~7,400/day is *demand met*, not a ceiling — 3.9's capacity math stands.
- **Harness health: normal, mid-rest.** `scoracle-cognition.service` was `inactive dead` at check
  time with `Result=success, NRestarts=0`, `ExecMainStartTimestamp=04:00:47`,
  `ExecMainExitTimestamp=06:02:02`; `scoracle-cognition-pause.timer` fired 06:00, resume fires
  07:00. That is the documented 2h-on/1h-off duty cycle (§0.6), not an outage. Running units:
  `scoracle-api.service`; watchers `scoracle-api.path` + `scoracle-cognition.path` **active
  (waiting)** — the §0.6 warning about builds into `go/bin/` and `rust/bin/` tripping a restart is
  live and real. Postgres and Ollama are **system** units; the harness is a **user** unit.

**0.11 — the Mac and its model ✅.** Mac = **`192.168.1.77`** (this box; en0). Ollama reachable
**from archbox**: `curl http://192.168.1.77:11434/api/tags` from archbox returns the model list.
**Exact Mistral 12B tag: `mistral-nemo:12b`** — 12.2B params, **Q4_0**, 7.07 GB, native
`context_length` 1,024,000, `embedding_length` 5,120 (so the plan's `num_ctx 4096` is a deliberate
budget, not a model limit). Also on the Mac: `ministral-3:14b` (13.9B Q4_K_M), `mistral-32k:latest`
and `mistral:latest` (both 7.2B). `rust/src/route.rs:620` already references `mistral-nemo:12b` on
MAC in its tests — the tag is consistent with the code. **On archbox:** `gemma3:4b` (4.3B, 3.34 GB)
✅ resident — the Editor's engine is already on the card's host — plus `qwen3:8b`, `mistral:7b`.
**No OLMo on either box**, consistent with the pre-execution audit removing it.
→ Appendix B's "exact Ollama tag" question is answered: **`mistral-nemo:12b`**.

### Handoff (phase 0 → 1)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phase 0 (ground truth) is committed — see its Log for measured baselines and any deltas.
Read §0 (protocol) and §1 (the packet contract), then execute Phase 1 (substrate migrations)
top to bottom. Migrations 200+; template sql/migration_template.sql; apply with sql/migrate.sh;
snapshot with scripts/hosting/snapshot-schema.sh; commit migration+snapshot together.
Everything in Phase 1 is inert — no code reads the new tables yet. Do not deploy anything.
Do not touch the legacy rail. STOP on surprise per §0.3.
```

---

## Phase 1 — Substrate (migrations 200+; all inert; no deploys)

Everything here is DDL that nothing reads yet. Safety comes from inertness, not caution.
Follow §1's column specs exactly; conventions from neighboring migrations (timestamptz,
`DEFAULT now()`, snake_case, COMMENT ON for every table).

- [ ] **1.1** Mig 200 `one_rail_storylines`: `storylines`, `storyline_articles`,
      `storyline_entities` per §1b. Indexes: `storyline_articles(article_id)`,
      `storyline_entities(entity_type, entity_id, sport, last_seen_at)`,
      `storylines(sport, status, last_seen_at)`.
- [ ] **1.2** Mig 201 `one_rail_editor_reads`: `editor_reads` per §1a (PK `article_id`, status
      CHECK = legacy taxonomy + `not_sport`), index on `(status, updated_at)`, GIN on
      `resolved` (jsonb_path_ops).
- [ ] **1.3** Mig 202 `one_rail_packets`: `packets` per §1c. Indexes:
      `packets(storyline_id, compiled_at DESC)`, `packets(day, sport)`, GIN `routing_tags`.
- [ ] **1.4** Mig 203 `persons`: `persons` table — `id serial PK, sport text NULL, full_name text
      NOT NULL, kind text CHECK (coach|executive|owner|agent|family|official|other), team_id int
      NULL, search_aliases text[] DEFAULT '{}', meta jsonb DEFAULT '{}', created_at`. (Kinds are a
      superset of `narrative_persons.kind`; that graph-layer table is unaffected and reconciles
      later — Appendix B.)
- [ ] **1.5** Mig 204 `person_entity_type`: extend `entity_type` CHECKs to admit `'person'` on
      `news_article_entities` and `entity_name_surfaces`; add `'candidate'` + `'fixture'` to
      `cognition_ledger`'s CHECK; add `'candidate'` to `pipeline_work`'s. (Only what v1 writes —
      no speculative admissions.) Pattern per CHECK: DROP CONSTRAINT, ADD CONSTRAINT ... NOT VALID,
      VALIDATE CONSTRAINT (row counts here validate in ms, but the pattern is the habit).
      **Do not** touch other entity_type CHECKs — only these four tables are on the rail.
- [ ] **1.6** Mig 205 `investigator_substrate`: the eight acquisition tables per the 2026-07-27
      planning doc, backend-named: `entity_candidates` (with `state` CHECK: `pending, accepted,
      rejected_not_sport, rejected_insufficient_evidence, rejected_out_of_scope, ambiguous,
      deferred_source_unavailable`; `idempotency_key text UNIQUE`; `norm_name`, `kind_hint`,
      `sport`, `target_entity_type/id`, `mention_count`, `first/last_seen_at`,
      `resolved_entity_type/id`, `decided_at`), `candidate_mentions` (candidate_id, article_id,
      quote, editor_descriptor, observed_at), `acquisition_runs` (candidate_id, status, query_plan
      jsonb, outcome, rejection_reason, model_version, parser_version, started/finished_at),
      `source_documents` (canonical url, final_url, domain, fetched_at, content_hash, http_status,
      title, retained_excerpt, headers jsonb), `entity_aliases` (entity triple, alias, norm_alias,
      source_document_id, state, **append-only** — enforce with a no-UPDATE trigger or a comment +
      review habit), `entity_external_ids` (entity triple, namespace, external_id,
      source_document_id), `entity_facts` (entity triple, fact_type, value_text, value_jsonb,
      valid_from/to, source_document_id, state CHECK active|superseded|conflicted),
      `entity_relationships` (subject triple, predicate, object triple, valid_from/to,
      source_document_id, state). A candidate, an attempt, a source, a fact, and a relationship
      are different things — combining them is how provenance disappears.
- [ ] **1.7** Mig 206 `packet_routing`: trigger `enqueue_voices_on_packet` AFTER INSERT ON
      `packets` — for each tag in `routing_tags` joined to `stage_routing_subscriptions`, insert
      `pipeline_work` rows per active `storyline_entities` participant of matching grain, with
      `input_version = 'pk:' || <that voice's slice_fingerprint>` (E2: unchanged slices do not
      reopen). Plus an unconditional `narratives` fan-out per participant (the Journalist reads
      everything). Follow the mig-197 trigger's ON CONFLICT/input_version pattern +
      `pg_notify('pipeline_work_ready','')`. **Ships live but fires into an empty subscription
      table + zero packets — doubly inert.** Do NOT seed subscriptions here (that is Phase 6; the
      mig-197 churn-loop lesson).
- [ ] **1.8** Extend `refresh_entity_name_surfaces()` to include `persons`
      (name + search_aliases, entity_type 'person') — mig 207. Run it; person surfaces = 0 rows
      today, which proves it's wired without changing behavior.
- [ ] **1.9** `scripts/hosting/snapshot-schema.sh`; commit migrations + snapshot together.

**Verify:** `select count(*) from storylines` etc. all return 0; `\d+ packets` matches §1c;
mig 206's trigger exists and `stage_routing_subscriptions` still has 0 rows; legacy pipeline
unaffected (`article_read` queue still draining — compare depth to Phase 0 Log).
**Commit:** `rail: phase 1 — substrate migrations 200–207`.

### Log (phase 1)
*(executor fills)*

### Handoff (phase 1 → 2)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–1 committed: substrate tables (storylines/packets/editor_reads/persons/acquisition)
exist and are inert; entity_type CHECKs admit 'person'; packet routing trigger is live but
fires into an empty subscription table.
Read §0 and §3, then execute Phase 2 (lungs — Go provenance, additive only).
One [DEPLOY] step at the end; deploys restart services via the .path watchers — that is
expected. Do not touch match.go or any regex path; deletions happen in Phase 8.
```

---

## Phase 2 — Lungs (Go; additive only; one small deploy)

The lungs mostly exist (teams-only Google News RSS sweep, 02:00 cron, `feed_rank`, the 0.95
primary link). This phase records what has been implicit and changes no behavior.

- [ ] **2.1** In `persistArticles` (`go/internal/thirdparty/news.go:319-329`), write query
      provenance into `news_articles.raw` on **insert only** (first-writer wins; on conflict leave
      existing): `{"q": <the literal query term used>, "lane": "primary|alias<N>", "edition":
      <ceid>, "window": "24h", "query_team_id": <id>}`. Thread the term/lane from
      `buildRSSSearchQueries` (`news.go:857`) through the fetch result to persist. The query IS
      the hypothesis — now it is also readable.
- [ ] **2.2** Add a funnel counter for articles that arrive with a body-bearing description vs
      empty (no behavior change; feeds Phase 3's fetch expectations).
- [ ] **2.3** Confirm (and note in Log) that the sweep is teams-only by design — "gather the broad
      topics of the sport" — and that persons/players are **never** swept; they enter via Editor
      discovery. This is doctrine, recorded here so nobody "helpfully" adds a player sweep.
- [ ] **2.4** `go test ./...`; build to a scratch path first; then **[DEPLOY]** `go build -o
      go/bin/pipeline ./cmd/pipeline` (watcher restart expected).
- [ ] **2.5** After the next 02:00 ingest (or a manual bounded run), verify:
      `select raw->>'q', count(*) from news_articles where fetched_at > now() - interval '1 day'
      and raw ? 'q' group by 1 order by 2 desc limit 10` returns sane query terms.

**Verify:** provenance present on ≥95% of new arrivals; article volume unchanged vs Phase 0
baseline (±20%); zero new Go errors in logs.
**Commit:** `rail: phase 2 — lungs record the hypothesis`.

### Log (phase 2)
*(executor fills)*

### Handoff (phase 2 → 3)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–2 committed: substrate inert, lungs now persist query provenance in news_articles.raw.
Read §0, §1a (the ep1 contract — you are implementing it exactly), §3, §4, then execute
Phase 3 (the Editor's junction, greenfield) top to bottom.
Phase 3.0 renames the LEGACY module junctions/editor/ -> junctions/article_reader/ (files only
— stage strings, env keys, prompt versions untouched, per §4 naming ruling) to free the
character's name for the new module. Key law: describe then derive (T2). Exact nrm() surface
match is the only automatic link path (T9). The new Editor writes editor_reads +
news_articles.full_text ONLY — it must not write news_article_entities, bucket, routing_tags,
or anything the legacy rail reads (shadow purity).
Fixtures: the legacy eval task renames editor -> article_reader WITH its fixtures dir in the
same commit (the dir MUST match the task name — a mismatch was once invisible for two days);
the new task takes the name `editor` with a fresh rust/fixtures/editor/.
```

---

## Phase 3 — Heart I: the Editor (greenfield junction, shadow mode)

New Rust stage `editor` in the character-named module `rust/src/junctions/editor/` — which the
legacy seat currently occupies, so the phase opens by renaming the legacy module to match its
own runtime identity (`article_read` stage, `Role::ArticleReader`). The new Editor fetches,
persists the body, reads on the ep1 contract, resolves in code, and writes **only** greenfield
tables. The legacy `article_read` keeps running untouched. Both share the 4-slot gemma group;
the new Editor registers FIRST so new arrivals outrank the legacy backlog (registration order
is claim order, `rust/src/main.rs:131-173`).

- [ ] **3.0** **The rename that frees the name (files only — §4 naming ruling).**
      `git mv rust/src/junctions/editor rust/src/junctions/article_reader` + update `use` paths;
      rename the legacy eval task string `editor` → `article_reader` in `eval_tasks.rs` AND
      `git mv rust/fixtures/editor rust/fixtures/article_reader` **in the same commit** (task
      name and fixtures dir must move together — `fixtures_dir()` resolves `fixtures/<task>`,
      and a mismatch once erred silently for two days). UNTOUCHED, verify by grep after:
      stage string `article_read`, `ARTICLE_READ_*` consts and env keys,
      `COGNITION_ROUTE_ARTICLE_READER`, prompt version `ar7`, table `news_article_readings`.
      Gate: `cargo test` at baseline AND `cargo run --bin eval -- --task article_reader
      --fixtures` passes all 51 checks (proves the renamed gate can still fire).
      Commit separately: `rail: phase 3.0 — legacy editor module renamed article_reader`.
- [ ] **3.1** Extract the fetcher into `rust/src/fetch.rs`: move `fetch_article` +
      Google-News URL resolution + headless-Chrome fallback + `clean_html` out of
      `junctions/article_reader/mod.rs` (formerly editor/mod.rs:816-880) into a shared module;
      the legacy module calls the extracted functions (mechanical refactor, behavior identical —
      run the existing test suite).
- [ ] **3.2** `Stage::Editor` (`"editor"`) in `work.rs` + `as_str` + claim ORDER BY
      `news_articles.feed_rank ASC NULLS LAST` (copy the ArticleRead arm at `work.rs:65-73`) +
      add to `KNOWN` in `main.rs:188`.
- [ ] **3.3** `Role::Editor` in `route.rs` (+ `Role::all()` array length bump + `env_suffix`
      `EDITOR`). Route config on archbox: `COGNITION_ROUTE_EDITOR=gemma3:4b` (§4: settled by
      hardware — no bakeoff scheduled).
- [ ] **3.4** `EditorHandler` in `rust/src/junctions/editor/`: `slot_group()` =
      `ARCHBOX_GEMMA_SLOTS`, `max_in_flight()` = 4, `rotation_batch()` = 8. Handle: if
      `duplicate_of` is already set (the mig-196 exact-title sweep), short-circuit — status
      `duplicate`, no fetch, no model call, no attach; the canonical carries the story (~5–12%
      of arrivals, free coverage). Otherwise fetch (via
      fetch.rs) → persist `news_articles.full_text` + fetch metadata → build ep1 prompt (system
      prompt: port the ar7 text minus the co_mentions instructions, plus `names[]` descriptors,
      `result_line`; **property order per §1a**) → gemma call (temperature 0.2, num_predict 900,
      num_ctx 8192, format_schema) → parse → **derive in code** (`editor/derive.rs`): relevance
      (port `derive_relevance` minus the co_mentions arm), resolver (exact `nrm()` lookup against
      `entity_name_surfaces`, ambiguity refused + recorded), nominations, routing tags,
      result_line parse → persist `editor_reads` (one tx) → ledger rows (`data_fetch_ledger` for
      the fetch AND `cognition_ledger` for the model call — closing the legacy seat's ledger gap;
      entity_type 'article').
- [ ] **3.5** Enqueue seam: in Go `persistArticles`, alongside the existing scrub enqueue
      (`news.go:412-421`), enqueue `stage='editor'` for every new article (same tx, same
      ON CONFLICT discipline). The Editor drains only where `COGNITION_STAGES` includes `editor`
      (archbox).
- [ ] **3.6** Eval task `editor` (fresh) in `eval_tasks.rs`, fixtures dir `rust/fixtures/editor/`
      (empty until this step fills it): port the 7 legacy fixtures from
      `fixtures/article_reader/` to ep1 expectations, then add: a coach-discovery case
      (kyle-shanahan shape), a place-collision case (Paris/Moulin Rouge — expect `descriptor`
      prevents the club link), a hallucinated-parent case (Fortuna Düsseldorf — expect
      exact-match refusal), a result-line case (verbatim score), an opponent-only case with the
      KEPT-since-ar6 expectation (`relevant=true` — the stale fixture trap), and a namesake tie
      (Vinicius — expect `refused_ambiguous`). Target ≥12 fixtures. The evaluator must run the
      **production** parser + derive path, as the legacy task's evaluate does
      (`eval_tasks.rs:1677`).
- [ ] **3.7** Tests + clippy at baseline; build target/debug; run
      `cd rust && cargo run --bin eval -- --task editor --fixtures` → 100%.
- [ ] **3.8** **[DEPLOY]** rust binary to archbox with `COGNITION_STAGES` += `editor`, the new
      Editor registered before article_read. **[DEPLOY]** the Go enqueue change. (Two
      watch-triggered restarts; do them together, outside a rest boundary.)
- [ ] **3.9** Shadow measurement, 48h minimum, recorded in the Log:
      (a) throughput and coverage: editor_reads/day vs arrivals/day. Capacity context (Scott,
      2026-07-29): the legacy ~7,400/day was DEMAND-limited, not a card ceiling — the harness
      runs 2h on / 1h off (HANDOFF §10; a 16h duty day) and Gemma sits mostly idle inside it,
      churning its queue fast. Arrivals (~8,140/day) should clear comfortably on the existing
      schedule, with idle headroom left over for the Investigator. Measure anyway — assumed
      headroom is how rails rot: record reads/day, p50/p95 queue wait, and GPU-busy fraction
      across two duty days. If coverage still lands <95% within 24h: (1) confirm the 3.4
      short-circuit is firing, (2) trim num_predict 900→750 (ep1 dropped co_mentions' output
      share), (3) STOP and surface — never silently accept the miss (§2 clause 1);
      (b) coverage: % of arrivals read within 24h (goal ≥95%);
      (c) status distribution (fetch_failed/paywall/blocked bands vs legacy's);
      (d) discovery: on a 50-article sample of players with known bleed (Olise-class), linked-in-
      `resolved` rate vs the legacy 39/182 Vinicius baseline;
      (e) resolver refusals/day (goes to the Investigator's future queue);
      (f) legacy rail health unchanged (narratives/day vs Phase 0 baseline — shadow purity).
- [ ] **3.10** `full_text` growth check vs disk headroom from Phase 0 Log.

**Verify:** 3.9 bands hold; fixture gate 100%; zero greenfield-Editor writes to any legacy-read
table (`news_article_entities`, `news_articles.bucket/routing_tags` untouched by the new stage —
assert by column-diff on a sampled day).
**Commit:** `rail: phase 3 — the Editor reads in shadow`.

### Log (phase 3)
*(executor fills)*

### Handoff (phase 3 → 4)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–3 committed: the greenfield Editor (stage `editor`, module junctions/editor/) reads
every arrival in shadow on the ep1 contract, persists bodies to news_articles.full_text and
reads to editor_reads, and touches nothing the legacy rail consumes (legacy module now lives
at junctions/article_reader/, eval task article_reader). See Phase 3 Log for measured
throughput/coverage.
Read §0, §1b–§1c (storylines/packets), then execute Phase 4 (storyline assembly + packet
compile — all deterministic code, zero model calls).
T3 is the law here: 0.5–0.75 similarity is the SAME STORY with a DIFFERENT CLAIM — attach,
never collapse. The disagreement is the story.
```

---

## Phase 4 — Heart II: storylines and packets (code, not calls)

- [ ] **4.1** `editor/storyline.rs`: the §1b attachment rule, invoked at the end of every Editor
      handle (after `editor_reads` persists): compute candidates → attach or open → write
      `storyline_articles`, upsert `storyline_entities` (join/`last_seen_at`), set
      `editor_reads.storyline_id`. Log every decision (storyline_id, score, candidate count) at
      debug level.
- [ ] **4.2** Backfill pass (one-shot bin, rehearsed rolled-back first): attach the shadow
      period's existing `editor_reads` (`attach_method='backfill'`, oldest first so storylines
      form in arrival order).
- [ ] **4.3** `editor/packet.rs` — compile on storyline-dirty with a 15-minute quiet debounce
      (drain-loop tick checks `storylines.last_seen_at`): assemble §1c from member
      `editor_reads` (claims from `key_facts` with article/source/published_at attribution —
      NO dedup across sources beyond byte-identical facts; register = strongest non-neutral;
      quotes code-sliced from `full_text`; slice fingerprints per voice: journalist = hash of
      claims+headline, vibe = hash of register+phrase+claims, transfers = hash of
      transfer-typed claims), insert packet (supersedes prior), mark storyline clean. Packets
      INSERT will fire mig 206's trigger — still inert (0 subscriptions).
- [ ] **4.4** Storyline lifecycle sweep (hourly, code): `open → dormant` after 14 quiet days;
      resolution stays manual/downstream for now (D5's close-in-one-stroke is wired but only
      invoked when a resolution is recorded — the transfer chain is the first writer, Phase 6).
- [ ] **4.5** Fixtures: storyline unit tests over canned editor_reads (the Real-Madrid-day
      shape: Diomande/Vinicius/Rodri/Lee Kang-in/Álvarez clusters — assert Lee Kang-in lands in
      its own storyline, NOT Real Madrid's, per the hand count).
- [ ] **4.6** **[DEPLOY]** rust to archbox.
- [ ] **4.7** Measure over 72h, in the Log: storylines/day/sport; articles-per-storyline
      distribution (the top cluster should land ~15–25:1 against the 20:1 hand count — outside
      that band, STOP and inspect attach scores); % of reads attached vs opened-new; hand-inspect
      the 3 biggest storylines for wrong merges AND for a preserved contradiction (T3 spot
      check — find one "agreed"/"not agreed" pair sharing a packet's claims).

**Verify:** 4.7 bands; packet trigger fired 0 work rows (subscriptions still empty).
**Commit:** `rail: phase 4 — storylines assemble, packets compile`.

### Log (phase 4)
*(executor fills)*

### Handoff (phase 4 → 5A)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–4 committed: packets are compiling in shadow; collapse ratio measured (see Phase 4
Log). Read §0, §4 (rulings), Appendix B decisions D-2/D-3, and the write-gate section of
wiki planning doc 2026-07-27 (cross-repo_living-database-seeker) if reachable — else §1 and
Phase 5A are self-contained. Execute Phase 5A (the Investigator — entity discovery, module
junctions/investigator/).
Laws: the Editor nominates, the Investigator verifies; search discovers, sources prove;
exact+discriminator or refuse — a name match alone NEVER merges identities; ambiguous is a
first-class outcome. No entity is written because a model knows a name.
```

---

## Phase 5A — Heart III: the Investigator (entity discovery)

Demand-led acquisition: the resolver's refusals and unresolved `names[]` become durable work;
verified people become `persons` rows the resolver can link tomorrow. (~60 coach-shaped
names/day measured in B3, plus the namesake ties roster context can't split.)

- [ ] **5A.1** Nomination sweep (code, in the Editor handle after resolve): for each unresolved
      name with `kind_hint='person'` (and for `refused_ambiguous` ties): upsert
      `entity_candidates` (idempotency_key = `nrm(name)||sport`; repeat mention bumps
      `mention_count`/`last_seen_at`, never duplicates) + `candidate_mentions` row with the
      code-sliced quote + descriptor. Clubs/national teams nominate too but with
      `kind_hint='club'|'national_team'` — they take the `rejected_out_of_scope` path in v1
      (Appendix B D-3) and stand as the census.
- [ ] **5A.2** Enqueue rule: candidate reaches `mention_count ≥ 2` distinct articles OR a
      refused-ambiguous tie → `pipeline_work (stage='investigate_entity',
      entity_type='candidate', entity_id=candidate.id)`. One-mention wonders wait (noise floor).
- [ ] **5A.3** `Stage::InvestigateEntity` + `Role::Investigator`
      (`COGNITION_ROUTE_INVESTIGATOR=gemma3:4b`, archbox base URL) + handler in
      `rust/src/junctions/investigator/`: slot_group = `ARCHBOX_GEMMA_SLOTS` (Scott's call: same
      card), `max_in_flight` = 1 (the Editor outranks it; register after the Editor). The card's
      idle time (3.9a) is expected to absorb this easily — raise the knob after 5A.10 if
      contention stays nil.
- [ ] **5A.4** Three adapters, kept separate (discovery ≠ retrieval ≠ interpretation):
      *Discovery*: (1) Wikipedia REST search+summary API (documented, structured, ToS-clean —
      the v1 workhorse for professional identity), (2) Google News RSS query for the name +
      sport term (reuses the lungs' client shape). *Retrieval*: `fetch.rs` with a per-domain
      budget (concurrency 1, ≥2s spacing per domain, respect 429/Retry-After, cache by
      canonical URL + content_hash into `source_documents` with a bounded `retained_excerpt`).
      *Interpretation*: gemma **describes** the page — `{page_says: {name_forms[], role, org,
      sport, league, nationality, dates[]}, quote}` — and CODE decides (5A.5). No browser
      automation in v1; a domain that blocks direct fetch is a domain we skip (never stealth).
- [ ] **5A.5** The write gate (`investigator/gate.rs`, deterministic): ACCEPT requires (a) ≥1
      `source_documents` row whose retained excerpt contains the name form, (b) sport-relevance
      from described role/org, (c) identity discriminator agreement (sport/league/team/role) —
      name similarity alone never merges (T9's cousin; do not rebuild BGE here); match against
      existing `players`/`persons` first — if an existing entity matches with discriminator,
      resolve the candidate to it (write alias, no new row). New person → `persons` row +
      `entity_aliases` (+ mirror into `entity_name_surfaces` via the mig-207 refresh or direct
      insert) + `entity_facts`/`entity_relationships` (e.g. `coach_of` with valid_from) each
      citing a source_document. Anything less → `ambiguous` (first-class, terminal until new
      evidence) or `rejected_*` with reason. `acquisition_runs` records every attempt +
      query_plan. Personal-life facts are NOT metadata (family kind exists for future editorial
      use; nothing auto-writes it in v1).
- [ ] **5A.6** Reopen policy: terminal candidates reopen only on a NEW distinct-article mention
      after `decided_at` + 30 days, or a manual reset. (No endless rediscovery loops.)
      Reopening an ACCEPTED candidate is the **maintenance loop**, not an error: re-verification
      re-runs the gate against current sources, supersedes changed relationships (`coach_of`
      closes with `valid_to`, the new one opens dated), and appends new aliases. The story that
      staled the fact is the story that fixes it.
- [ ] **5A.7** Adversarial fixture set `rust/fixtures/investigate_entity/` from the B3 census:
      kyle shanahan (accept: coach, NFL, 49ers), xabi alonso (accept: coach — despite an
      ex-player record shape), pep guardiola (accept coach; must NOT merge into player `sergi
      guardiola`), spain (out-of-scope national team → census, no write), celtic (out-of-scope
      club), andy burnham / lee child / ice (rejected_not_sport), vinicius tobias vs junior
      (discriminator split or ambiguous — never a coin flip). Gate: 100%.
- [ ] **5A.8** Review surface: SQL views `investigator_review_accepted` (latest 50 with sources),
      `investigator_funnel` (counts by state/kind/day). Sampling protocol in the Log: 20 accepted
      hand-checked; **one false merge is a stop-the-line event** (blocks widening any gate until
      explained + regression-fixtured).
- [ ] **5A.9** Tests, fixture gate, **[DEPLOY]** rust to archbox (`COGNITION_STAGES` +=
      `investigate_entity`).
- [ ] **5A.10** Measure over 72h in the Log: nominations/day, candidates by state, acceptance
      rate, editor-slot contention (Editor coverage from Phase 3.9 must not degrade >5%), and
      the compounding metric: resolver links landing on `persons` rows (starts ~0, should grow
      as accepted coaches recur).

**Verify:** 5A.8 sample clean (0 false merges); Editor coverage held; funnel view populated.
**Commit:** `rail: phase 5A — the Investigator verifies people`.

### Log (phase 5A)
*(executor fills)*

### Handoff (phase 5A → 5B)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–5A committed: entity discovery is live in shadow — coaches and unresolved names
become verified persons rows with provenance. Read §0, §3, Appendix B D-4 (pilot sport +
source family — confirm Scott has decided, else STOP and emit the block asking him), then
execute Phase 5B (box scores — the stats rail reborn).
Laws: one adapter per source family; DOM/JSON parsing, never regex, never LLM number-reading;
fixture identity validated before anything writes; a correction is a revision + recompute,
never an overwrite. The landing table fixture_boxscore_fetches and stage fixture_boxscore
already exist (mig 189 / rust/src/boxscore_fetch.rs) — you are replacing their SOURCES
(providers are cancelled) and moving the file under junctions/investigator/, not changing
their shape. The stage wire name fixture_boxscore is a live queue identity — it does NOT
rename (§4 naming ruling).
```

---

## Phase 5B — Heart IV: box scores (the collision, made real)

Third-party stats are gone (cancelled 2026-07-27/28). The news rail now *causes* the stats
rail: the Editor's `result_line` nominates a completed fixture; the Investigator scrapes,
validates, and promotes it; the existing triggers (derived stats, `detect_team_change`,
tier/PEAK/momentum enqueues) fire off those writes exactly as they did off the provider seeder.
`player_team_history` comes back to life for free.

- [ ] **5B.1** `boxscore_sources` table (mig, data-not-code): sport, league_id NULL, domain,
      discovery mode (url_template | search), parser_family, trust_state
      (candidate|trusted|suspended), fetch_policy jsonb (rpm, concurrency, cache_ttl). Seed the
      Appendix B D-4 pilot family only.
- [ ] **5B.2** Fixture nomination (code, Editor handle): parseable `result_line` + both teams
      resolved → match `fixtures` within ±2d on (sport, home/away or reversed) → if found and
      status ≠ completed, or scores differ, or no row: upsert a fixture row
      (status='completed', scores from the parse, `external_id` NULL — Scoracle identity, not
      provider identity) flagged `meta needs_verification` → enqueue `fixture_boxscore`.
      Rehearse the upsert rolled-back against a real day first; fixture identity errors here are
      the highest-severity failure of the phase.
- [ ] **5B.3** Move the box-score stage into the character's module: `git mv
      rust/src/boxscore_fetch.rs rust/src/junctions/investigator/boxscore.rs` (+ `use` path
      updates; the stage wire name `fixture_boxscore` and the landing table are live identities
      and do NOT rename). Then extend it: `SourcePlan` from `boxscore_sources` (replacing the
      provider map path for the pilot sport), retrieval through the 5A budgeted fetcher into
      `source_documents` + the existing `fixture_boxscore_fetches` landing row, one DOM/JSON
      parser module per source family. A model may TRIAGE an unfamiliar page layout
      (describe-only) — it never reads numbers into rows.
- [ ] **5B.4** Validation gate (code): fixture identity (teams, date, competition), final
      status, participant completeness vs known rosters (warn-level, not fatal — rosters drift),
      per-stat key mapping into `stat_definitions.key_name` for the sport (unmapped keys land in
      `raw_labels`, never guessed), arithmetic checks (totals vs sums where the sport defines
      them), source revision (content_hash change on refetch → revision, not overwrite).
- [ ] **5B.5** Promotion (the old seeder's job, now gated): validated landing row →
      `event_box_scores` + `event_team_stats` + fixtures scores/status in one tx, parser_version
      stamped. Downstream fires by itself (verify, don't re-trigger): derived-stat BEFORE
      triggers, `trg_detect_team_change`, tier/PEAK/momentum enqueue paths.
- [ ] **5B.6** **The replay gate — the phase's proof.** Pick 20 provider-era completed fixtures
      (pilot sport) with stored `event_box_scores`. Run the public-source path end-to-end into a
      rolled-back tx; diff shared stat keys vs provider rows. Gate: 20/20 fixture identity, ≥95%
      shared-key agreement (document every disagreement — some will be provider errors; that is
      the finding, not a failure).
- [ ] **5B.7** Fixtures for the parser family (3 canned pages incl. one malformed), tests,
      **[DEPLOY]** rust to archbox (`fixture_boxscore` already in `COGNITION_STAGES` from the
      provider era — verify).
- [ ] **5B.8** Run live for 7 days on the pilot sport, in the Log: fixtures nominated/verified
      per day, validation failure taxonomy, promotion count, `detect_team_change` firings,
      PEAK/momentum enqueues caused, and one Scout (`peak`) read post-promotion sampled for
      sanity (the Scout is now reading scraped-truth z-scores — the collision achieved).

**Verify:** replay gate passed; live week promoted >0 fixtures with 0 identity errors; Scout
read sane.
**Commit:** `rail: phase 5B — box scores from public sources`.

### Log (phase 5B)
*(executor fills)*

### Handoff (phase 5B → 6)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–5B committed: the heart is whole — the Editor reads, packets compile, the
Investigator verifies people and box scores; stats flow without providers. Read §0, §1c
(slice fingerprints), §2 (you are wiring the seam the cutover will flip), §3 (Mac topology,
from Phase 0 Log 0.11), then execute Phase 6 (the brain — voices onto packets, behind
RAIL=legacy).
Laws: no prose reaches the Scout (T4); the packet renderer's 4096 budget is HARD; voice
routing is data (subscriptions), never asked of a model (E1); nothing user-visible changes
until Phase 7 flips RAIL.
```

---

## Phase 6 — Brain: the voices read packets (wired, not yet live)

Everything lands behind `RAIL` (env, default `legacy`). Legacy behavior is bit-identical until
Phase 7 flips it.

- [ ] **6.1** `RAIL` config (`rust/src/config.rs`): `legacy|packet`, read once at boot, logged
      loudly at startup.
- [ ] **6.2** Packet renderer (`editor/render.rs`): `(packet, entity, voice) → context block`,
      hard budget ≤2,800 tokens (tiktoken-free heuristic: chars/3.6, then assert with
      `eval_count` telemetry): headline, this entity's role + participation dates, claims
      (attributed, contested pairs marked `⇄`, newest first, truncate oldest first), register +
      phrase (Influencer render only), facts, one continuity line from the prior packet.
      Property-based test: NO packet in the shadow corpus renders >2,800 for any voice.
- [ ] **6.3** Journalist: `load_packet_corpus` beside `load_vetted_corpus_with_exclusions`
      (`journalist/mod.rs:380`), selected by RAIL inside `load_narratives_material`
      (`journalist/mod.rs:1078`). Packet path: entity's packets from the last 72h, rendered,
      `num_ctx` 4096, `num_predict` 700, exclusions telemetry intact (every packet dropped by
      budget is named — the A5 rule).
- [ ] **6.4** Seed `stage_routing_subscriptions` (the E1 INSERT, packet-grain):
      `('transfer','transfers','team')`, `('charged','vibe','*')`. The Journalist needs no row
      (mig 206 fans narratives unconditionally). **Do not** also leave mig-175's article-grain
      transfers trigger pointing at the same stage post-flip — Phase 7 drops it; until then the
      RAIL gate in the transfers handler ignores packet-work rows under legacy (input_version
      prefix `pk:` is the discriminator).
- [ ] **6.5** Insider (`transfers` handler): RAIL=packet path reads the packet render (its
      slice = transfer-typed claims) instead of the article-window query; the
      `transfer_identity_applications` adjudication chain downstream is UNTOUCHED (it is kept
      substrate, news-derived, and the Scout's road).
- [ ] **6.6** Influencer (`vibe`): E3 — under packet RAIL she wakes from the packet trigger
      (`charged` tag), first-voice-capable: fix `enqueue_vibe_if_needed`'s empty-context no-op
      for packet work, update her contract text (she may file first; register_phrase is her
      material). The Journalist-side enqueue remains for legacy mode only.
- [ ] **6.7** Scout (`peak`): NO packet subscription (T4). Two confirmed-fact roads only:
      (a) the stats platform (now Investigator-fed); (b) `transfer_identity_applications`
      applied/adjudicated rows — add a compact "personnel changes since last read" block to the
      PEAK context from that table (facts with dates, no prose). Injury/suspension confirmation
      gates (the F4 pattern: claims → threshold → confirmed) are **deferred** to post-cutover
      (Appendix B D-5) — do not improvise them here.
- [ ] **6.8** Analyst + Oracle: Analyst's context assembly gains the packet render under RAIL
      (peer-aware inputs unchanged). Oracle mechanics untouched — but implement E5 while we are
      here: when deterministic `pillar_convergence` < 40, the prompt hands the divergence as a
      decided fact and `DISAGREEMENT:` becomes a REQUIRED field (grammar-enforced), narrating
      it. One fixture proves it fires (a guard never observed firing is not a guard).
- [ ] **6.9** Mac routing config (values from Phase 0 Log 0.11): for the six voice roles,
      `COGNITION_ROUTE_<ROLE>_BASE_URL=http://<mac>:11434`, models per Appendix B D-1 (the
      Mistral 12B tag), `COGNITION_BACKEND_CONCURRENCY=http://<mac>:11434=3,http://localhost:11434=4`,
      voice `num_ctx` stays 16384 under legacy / 4096 under packet (RAIL-scoped constant —
      change `VOICE_NUM_CTX` const to a RAIL-aware fn). Mac worker runs voice stages only
      (`COGNITION_STAGES` on the Mac).
- [ ] **6.10** Voice fixture refresh: capture packet-context fixtures via
      `eval --capture-ledger` on shadow renders for narratives/vibe/transfers; gates green on
      BOTH rails (legacy fixtures must still pass — nothing changed under legacy).
- [ ] **6.11** Tests, clippy, **[DEPLOY]** rust to archbox AND the Mac worker, RAIL=legacy
      everywhere. Verify boot logs on both machines print rail + routes + backend budgets.
- [ ] **6.12** Dry-run under eval (not production): run each voice's packet path against 5
      shadow packets on the Mac; record p50/p99 prompt tokens (≤3,200), output sanity, Mac
      concurrent-3 sustained without runner reloads (uniform 4096 num_ctx per host — mixed
      num_ctx forces reloads, `route.rs:52-75`).

**Verify:** legacy production metrics unchanged over 48h post-deploy (T5 says gates can't see
this — compare production rates to Phase 0/3 baselines); packet dry-runs within budget; both
fixture gates green.
**Commit:** `rail: phase 6 — the brain is wired for packets`.

### Log (phase 6)
*(executor fills)*

### Handoff (phase 6 → 7)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–6 committed: the whole new rail runs in shadow; voices are wired for packets behind
RAIL=legacy; Mac serves the six voices. Read §0 and ALL of §2 (the cutover condition — you are
measuring it), then execute Phase 7. Phase 7 does NOT flip anything until the 7-day condition
is green and Scott has said "flip" — the flip is his act, prepared by you. If any clause fails,
STOP, log the numbers, emit the block with BLOCKED.
```

---

## Phase 7 — The cutover

- [ ] **7.1** Write the five §2 condition queries as `scripts/rail-cutover-check.sh` (read-only,
      prints PASS/FAIL per clause with numbers; runnable daily by cron or by hand). Clause SQL
      sketches: (1) `editor_reads` within-24h coverage vs arrivals; (2) legacy corpus
      entity-days LEFT JOIN packet entity-days → 0 missing; (3) emit the day's 50-link audit
      sample as a table for hand/model review, record the score; (4) `eval --task editor
      --fixtures` + dead-letter count on editor/investigator stages; (5) packet-claims
      referential check + exclusions accounting.
- [ ] **7.2** Run it daily for 7 consecutive days; paste each day's output into the Log. Any
      FAIL resets the window (and is a finding — fix, then restart the count).
- [ ] **7.3** Prepare the flip-day migration (do not apply until flip): mig 2xx
      `retire_legacy_rail_triggers` — DROP `enqueue_derive_on_vetted` (T10 dies with it) and
      `enqueue_transfers_if_transfer_related`; leave `enqueue_voices_on_routing_tags`
      (article-grain, subscription table now serves packet grain; trigger fires into no
      article-grain rows — dropped in Phase 8). The Appendix A revert block is the rollback.
- [ ] **7.4** Prepare the flip-day Go change (do not deploy until flip): `persistArticles` stops
      enqueueing `scrub` (editor enqueue stays); regex secondary-link loop behind a
      `LEGACY_LINKS=0` env default-off (deletion is Phase 8; off is enough for flip day).
- [ ] **7.5** Prepare the Editor's link-writing switch (RAIL=packet side): the Editor begins
      writing `news_article_entities` rows for its resolved links (vetted=TRUE,
      `match_confidence=0.90` sentinel distinct from 0.95/0.8 so Editor links stay greppable),
      confirming/denying the 0.95 hypothesis link per `entity_roles`. Ordering on flip day
      matters: **triggers drop BEFORE the Editor writes vetted** (T10).
- [ ] **7.6** **FLIP (Scott's act, one sitting):** apply 7.3 migration → [DEPLOY] Go (7.4) →
      [DEPLOY] rust with RAIL=packet on archbox + Mac, `COGNITION_STAGES` drops
      `article_read` + `scrub` on archbox, voice num_ctx 4096 on Mac → run
      `rail-cutover-check.sh` once more against the live flip → snapshot-schema; commit.
- [ ] **7.7** Watch 48h with point-in-time checks (not watchers): packets/day, narratives/day
      (expect a T7 step change — packets collapse coverage-volume into story-volume; the OLD
      baselines are not comparable, record the new ones), vibe first-voice firings AND total
      vibe volume (the charged gate thins her cadence by design, but momentum's `vibe_slope`
      must not starve — vibe samples down >70% vs legacy is a finding to surface, answered by
      widening the `charged` derivation or a reconcile tick: decided, not drifted), transfers
      packet-work drains, Editor coverage, Investigator funnel, Mac throughput. Rollback
      trigger: any voice starving >6h or Editor coverage <80% → RAIL=legacy (env flip +
      Appendix A trigger revert), diagnose cold.

**Verify:** 48h stable on the new rail. **Commit:** `rail: phase 7 — cutover; the old rail is
off`.

### Log (phase 7)
*(executor fills — including all 7 daily condition outputs and the flip-day timeline)*

### Handoff (phase 7 → 8)
```
Resume PLAN-one-rail.md in scoracle-backend (Scoracle greenfield rail).
Phases 0–7 committed: RAIL=packet is live; the legacy rail is OFF (triggers dropped, scrub +
article_read unscheduled, regex links disabled). 48h stability is in the Phase 7 Log.
Read §0 and Appendix A (the demolition inventory), then execute Phase 8: delete the corpses,
rebaseline what T7 says moved, update the wiki, write the closing handoff. Deletion only —
if removing something changes a passing test's behavior, STOP: that thing was not a corpse.
```

---

## Phase 8 — Demolition, rebaseline, and the record

- [ ] **8.1** Execute Appendix A top to bottom (Go, Rust, SQL, cron, env). Each bullet is its
      own commit or tight group; `go test ./...` + `cargo test` green after every group.
- [ ] **8.2** Freeze legacy artifacts as archive: `news_article_readings`,
      `narrative_threads`, old fixture dirs (move `rust/fixtures/article_reader/` →
      `rust/fixtures/_retired_article_reader/`) — COMMENT ON the tables as retired; no drops
      (the archive is the moat).
- [ ] **8.3** Rebaseline (T7): 7-day fresh baselines for narratives/day, card_score
      distribution, momentum enqueue rate, transfer heat volume (player links now Editor-written;
      proximity gates lenient) — recorded in the Log as the new normal. The old numbers are
      history, not targets.
- [ ] **8.4** Wiki updates (scoracle-wiki): Living Database status → shipped-for-people+scores,
      Seeker → Investigator rename note; AI Stage Conventions → stage table gains
      editor/investigate_entity (+ Editor/Investigator contracts, the packet as the narratives
      corpus, VOICE ctx 4096, two-host topology, the character-named module convention);
      Archbox Infrastructure → cron changes; DATA_FLOW.md + RUNBOOK.md in this repo likewise.
      Docs and code disagree → the code wins; make the docs agree.
- [ ] **8.5** Crontab: remove `cron-narrative-links.sh` (co-mention refresh); confirm 02:00
      ingest, tier recompute, backups unchanged; add `rail-cutover-check.sh` renamed
      `rail-health-check.sh` weekly.
- [ ] **8.6** Write `HANDOFF-one-rail.md`: what shipped, the new baselines, the open decisions
      (Appendix B leftovers: F4 injury gates, national teams, out-of-scope clubs, the front
      page), and mark this plan **DONE** in the STATE line.

**Commit:** `rail: phase 8 — demolition complete; one rail`.

### Log (phase 8)
*(executor fills)*

---

## Appendix A — Demolition inventory (execute in Phase 8; prepared by recon 2026-07-28)

**Go** (the judging tier; keep the clerk):
- `go/internal/thirdparty/match.go` — delete all EXCEPT `isTeamEntity` (move it beside its
  callers in `news.go`). `SportContextTerms` goes; the `sportTerms` map in `news.go:55` STAYS
  (query builder needs it).
- `news.go`: secondary-link loop `:363-392` (+ its `LEGACY_LINKS` switch from 7.4),
  `articleMatchText :1213`, `posOrNil :548`, entity pool `:149-163, :181-182, :285-305,
  :437-532`, `BackfillTitlePositions :563-635`. Scrub enqueue block (already off since flip).
- `go/cmd/comention-backfill/` — whole directory.
- `funnel.go` MatchRejected counter + tests touching it; `maintenance.go:554-569` dead auto-vet;
  `maintenance.go` scrub backstop sweep + `NEWS_SCRUB_ENABLED` config.
- Tests: `news_test.go` regex cases, `news_live_test.go`.

**Rust:**
- Stage `article_read`: the whole `junctions/article_reader/` module (renamed from
  `junctions/editor/` in 3.0; fetch already extracted to `fetch.rs`) — handler, co-mention
  verdict application, vetted clearing, `derive_relevance`, bucket + routing_tags writers.
  `junctions/editor/` (the character's module) is the survivor.
- Stage `scrub` (`rust/src/scrub.rs`) — vetted is the Editor's fact now.
- `Role::ArticleReader` + `COGNITION_ROUTE_ARTICLE_READER` (env removed on BOTH machines —
  coordinated config change), `Role::all()` shrinks.
- BGE/embedder + `threads` cosine clustering in the narratives path (the novelty gate's
  embedding branch; exact-title dedup mig 196 + URL dedup STAY — they are deterministic).
  `narrative_threads.centroid` stops being written.
- `ARTICLE_READ_*` consts, `ar*` prompt-version namespace (T1 retires with the cache it keyed).
- Eval task `article_reader` (renamed in 3.0; the task name `editor` now belongs to the
  greenfield junction) — unregister it; retire its fixtures dir per 8.2.

**SQL** (migration `2xx_demolition`, after 7-day rollback window):
- DROP trigger `enqueue_voices_on_routing_tags` (article-grain; packet-grain trigger from mig
  207 is the survivor) + function.
- DROP functions `refresh_co_mention_links(...)`, `enqueue_derive_on_vetted()`,
  `enqueue_transfers_if_transfer_related()` (triggers already dropped at 7.3).
- `news_articles.bucket` — stop-write already happened (the greenfield Editor never wrote it);
  column stays, commented retired (archive).
- `news_article_entities.title_pos` — stays as historical data, commented retired.
- Recorded revert for the 7.3 trigger drops (rollback window only — delete this block in 8.1):
  re-CREATE `enqueue_derive_on_vetted` + `enqueue_transfers_if_transfer_related` from
  `sql/schema/schema.sql` @ the pre-flip snapshot commit.

**Cron/env:** `cron-narrative-links.sh` out of crontab; `NEWS_SCRUB_ENABLED`, `LEGACY_LINKS`,
legacy route envs removed from both machines' unit env files.

---

## Appendix B — Decisions Scott owns (defaults act if he is silent, except D-4)

- **D-1 · Voice model tag.** Scott: "Mistral 3:12b" on the Mac. Phase 0.11 records the exact
  installed Ollama tag; Phase 6.9 routes it. Routes are config — a different tag is an env
  edit, and any *model change* re-earns its seat on the voice fixtures (AI Stage Conventions).
  **ANSWERED by Phase 0.11 → `mistral-nemo:12b`** (12.2B, Q4_0, 7.07 GB, native ctx 1,024,000),
  installed on the Mac (192.168.1.77) and reachable from archbox. `rust/src/route.rs:620` already
  names that tag on MAC, and it is the only 12B on the box — so the plan's "Mistral 12B" is this.
  ⚠️ *One ambiguity for Scott, harmless until 6.9:* the Mac also carries **`ministral-3:14b`**
  (13.9B, Q4_K_M), which matches the "**3**" in "Mistral 3:12b" while `mistral-nemo:12b` matches
  the "**12b**". Default stands as `mistral-nemo:12b` (it is the 12B, and the code already
  references it); if Scott meant the Ministral, Phase 6.9 is a one-line env edit.
- **D-2 · Person kinds v1** default: coach, executive, owner, agent, official (auto-writable);
  family exists in the enum, never auto-written.
- **D-3 · Out-of-scope clubs + national teams** default: census only (`rejected_out_of_scope`),
  no auto-writes. When Scott widens the boundary, `teams.kind` (club|national) is a two-minute
  migration — deferred deliberately, since a column nothing writes is scar tissue. The boundary
  is a business decision, not a resolver decision.
- **D-4 · Box-score pilot: sport + source family. NO DEFAULT — Phase 5B blocks on this.**
  Recommendation: FOOTBALL (78% of volume) with one reputable structured source family after a
  terms/robots review in 5B.1; NBA is the fallback (smallest surface, cleanest tables).
- **D-5 · Injury/suspension confirmation gates (F4 pattern)** default: deferred post-cutover;
  the Scout reads stats + transfer confirmations until then. (Scott flagged interest —
  schedule as its own mini-plan after Phase 8.)
- **D-6 · The front page.** Audited out of the critical path: the packets are the compiled
  stories of the day, and a ranking call with no client surface is decoration (a junction that
  only decorates is noise — AI Stage Conventions; this is a product, not a junction, but the
  rule's spirit applies). When Scott picks a surface, the package is a `front_pages` table +
  one day-close pick-by-number call per sport (`Role::Editor`, D3 closed-list discipline) —
  about a half-day of work sitting behind the decision.

---

## Appendix C — What this plan deliberately does not do

- No packet backfill over the 150,566-article archive (forward-only; the archive keeps its old
  readings — the lazy-invalidation finding says they were never coming back anyway).
- No BGE revival, no semantic-similarity identity authority, anywhere (T9, write-gate law).
- No browser automation in Investigator v1 (direct fetch + APIs; a blocked domain is a skipped
  domain).
- No player/person RSS sweeps (teams carry the sport's story surface; discovery finds the
  people).
- No new public reading junction: the Editor and the Investigator are supporting characters —
  the six-voice cast and the one-card-per-character surface are untouched (Characters doctrine).
- No schedule scraping in v1 — completed games enter by news demand (`result_line`). Thinly
  covered fixtures can be missed; provider-era `fixtures` rows stuck `scheduled` are the running
  census of what demand alone doesn't catch, and the signal for a v2 schedule source.
- No renames of live legacy identifiers (F1's lesson, §4 ruling) — files take character names;
  stage strings, env keys, prompt versions, and tables keep theirs until demolition.

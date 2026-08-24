# The availability tag, and the turn to public sources

**Session of 2026-08-23 (evening).** Branch `availability-record-and-public-source-turn`, six
commits, nothing pushed. Everything below was verified against production unless marked
otherwise.

This session started on the AI-Investigator handoff (`fixture_boxscore` is dead in prod) and
ended somewhere else, because two of Scott's rulings mid-session changed the shape of the work.
Both turns are recorded here, including what they retired.

---

## 1. What landed on production

| Migration | State |
|---|---|
| 228 — `stat_summaries.trigger_type` widening | **APPLIED** |
| 229 — `player_availability` | **APPLIED** |
| 230 — seeder-era provider-map demolition | **APPLIED** |
| 231 — Scout availability routing | **VALIDATED, NOT APPLIED** (see §7) |

Ledger head is `230_seeder_era_provider_map_demolition`. Each was validated inside a rolled-back
transaction first, individually and as an ordered sequence, and prod was confirmed untouched
before the real apply. `sql/schema/` was re-snapshotted (commit `0340bf4`) — the dump ran ON
archbox because there is no `pg_dump` or `psql` on the Mac, and the script's
`DATABASE_PRIVATE_URL` path expands empty off the systemd units, the same trap the handoff
records for psql-over-ssh.

**228 disarmed a live latent defect.** `RatingHandler` has written `trigger_type = 'transfer'`
since `615bdcb` (2026-08-15) against a CHECK that admitted only stat_change/periodic/manual. It
had cost nothing only because all five all-time applied transfers landed in July, before the
trigger shipped.

**230 dropped `provider_fixture_map`** (29,537 rows) with its trigger, its enqueue function and
the dead `resolve_provider_fixture_id`, and re-issued the fingerprint `fbf1:` → `fbf2:`. Data
dumped first to `archbox:~/scoracle_backups/provider_fixture_map_20260823.sql` (29,537
column-INSERTs, count-verified), so the forfeit is recoverable.

> **One drafted comment in 230 was wrong and is corrected in the file.** It claimed zero rows for
> season 2026; there were **272** — all `scheduled` NFL fixtures kicking off 2026-09-09 ..
> 2027-01-10, written in one batch on 2026-06-22 at schedule release. None was completed, so none
> was ever a box-score candidate, and the claim that actually matters (zero of the 159 queued
> fixtures had a mapping) held. The file now states what prod states.

Queue integrity checked rather than assumed: the conflict key is
`(stage, entity_type, entity_id, sport)` with no `input_version`, so the 159 pending rows update
in place to `fbf2:` and mig 225's FIFO clause preserves their 2026-08-14 `available_at`. No
duplication.

---

## 2. Retrieval is wired (commit `c45fb6d`)

`fetch.rs:516` says `BudgetedFetcher` was "founded in Phase 4 (box scores), reused by Phase 5" —
but this seat never used it, going direct to the two vendor clients instead. So the substrate
founded FOR this path had only ever served entity discovery. It is wired back:

- `select_source` reads `boxscore_sources` (mig 208) instead of returning a hardcoded nothing.
- `fetch_source` goes through the fetcher and owns no HTTP client: per domain, concurrency 1, the
  2s floor, 429/Retry-After as a hold, the breaker at four failures, a `source_documents` row per
  retrieval with `cache_ttl` reuse.
- A held domain and a `403` are both `blocked` and both TERMINAL — *"a domain that blocks direct
  fetch is a domain we skip, never stealth."* Re-queueing would be the stage arguing with the
  budget that exists to stop it.

**Three eligibility screens, exercised against prod in a rolled-back transaction:** `suspended`
excluded but never deleted; league-specific rows outranking sport-wide; and — the screen easiest
to omit — a source is ineligible unless `terms_review->>'verdict' = 'pass'`. A discovery arm that
proposes domains must not be able to make one fetchable by INSERT alone. Verified that a row with
an empty `terms_review` is excluded even while `trust_state` says `trusted`.

**The registry is still empty, so live behaviour is unchanged.** What changed is that the
emptiness is the DATA's now, not the code's.

---

## 3. Source research — the honest verdict

Scott chose "open-licensed football data". Checked against three bars: licence, current-season
coverage of the five leagues, and whether per-player lines exist at all.

| Source | Licence | 2026 top-5 | Player data | Verdict |
|---|---|---|---|---|
| StatsBomb open-data | free w/ attribution | **No** (newest: Bundesliga 23/24, PL 15/16) | rich | historical corpus — out |
| openfootball `football.json` | **CC0** | yes | **none** — date/teams/score only | duplicates `fixtures` — out |
| football-data.org | none published | yes | tier-gated | 403s without a key; a free TIER, not an open LICENCE — out |
| **OpenLigaDB** | **ODbL**, keyless | partial | goalscorer + minute + pen/OG | only candidate |

OpenLigaDB caveats: crowdsourced (the league list contains "Moisty Mire League" and a misspelled
"Permier League England"), covers Bundesliga/LaLiga/PL for 2026 with **no Serie A and no Ligue
1**, and its PL had 1 finished match against our 36.

**Conclusion: no open-licensed source carries current-season per-player box scores for the five
leagues.** OpenLigaDB's real value is as ODbL ground truth for FIXTURE IDENTITY, not as a
box-score source.

---

## 4. The fixture spine is contaminated — the session's biggest finding

All **174** completed FOOTBALL 2026 fixtures have `external_id IS NULL`; FOOTBALL 2025 has one on
all 1,752. Real ingest **stopped in June** (newest external_id: FOOTBALL 2026-06-15, NFL
2026-06-22, NBA 2026-05-30). Everything created since is an Editor nomination — `completed` count
is **289**, exactly the nomination count across all three sports. Real fixtures are `seeded`.

Root cause is BY DESIGN at `editor/nominate.rs:206`: the Editor mints fixtures from NEWS
ARTICLES, where `start_time` is the article's publish timestamp ("the article anchor"), `status`
is hardcoded `'completed'`, and the score is parsed from prose.

Measured:

- **Chelsea "played 5 matches" on 2026-08-03.** 35 team-days double-booked across 174 fixtures.
- Kickoffs at 09:27 and 13:17 — publication times, not football slots.
- `round` NULL on all 174; 2. Bundesliga clubs tagged Bundesliga; 7+ duplicate pairs.
- **All 159 queued box-score rows target these.**

That code's own comment says *"The Investigator's verified fetch revises start_time"* — the
corrector IS the disabled stage, which is why the pile accumulated. Only 36 of 174 were revised.

**Two consequences for the build:**

1. A `{date}` URL template renders the ARTICLE's publish date, not kickoff, so date-keyed
   templates systematically miss for exactly the fixtures in the queue.
2. **The "free correctness oracle" is weaker than the handoff assumed.** §6 says scores are
   "known independently" — true for provider-seeded 2025 fixtures, false for these: the score came
   from a news article. Reconciling against it cross-checks two news-ish ecosystems; it is not
   independent ground truth.

---

## 5. The Scout's availability READ (commit `a8354a0`)

`load_personnel_changes` selected transfers only, so a correctly-woken Scout — debounce bypassed,
model call paid for — arrived at a card with zero availability facts, and s21 correctly forbade
him inventing any.

`AvailabilityChange` + `load_availability_changes` + a widened `render_personnel_block`. A
SEPARATE struct from `PersonnelChange`, the same judgement mig 229 made in the schema: a transfer
is a MOVE, an availability event is a SPAN. `returned` (the player came back) and `reverted` (the
record was wrong) render as different sentences with a test asserting neither can read as the
other.

T4 verified against prod by planting `"PROSE THAT MUST NOT LEAK"` in `revert_reason` in a
rolled-back transaction and confirming it does not appear in the read.

---

## 6. THE TURN: Scott's ruling retires the structured path

Mid-session, verbatim:

> *"I think this is a little complicated. Too much rigidity. How I envision it: Editor notices
> injury/suspension and tags the Scout → the Scout decides the legitimacy of the report → event is
> included in the report."*

and:

> *"I'd like to empower each model. Guards over evals. We can let the model do the work versus
> trying to engineer a rigid process."*

**What this RETIRED**, all of it work this session had scoped or started:

- the `player_availability` WRITER;
- a new Editor contract field naming the injured party (which had been diagnosed as blocking,
  because `story_type` is one enum for the whole article and `entity_roles` only carries
  `subject|opponent|passing_mention|absent`);
- the Rust-side enqueue helpers as the primary path.

**What replaced it** (commit `c158c0c`) — four small changes, no new table, no contract field, no
status machine:

1. **`packets.slice_fingerprints` gains a `rating` key** hashing injury/suspension claims, the
   same shape as the Insider's `transfers` slice.
2. **`rating_work_bypasses_debounce` accepts `pk:`** versions as non-statistical.
3. **`Voice::Scout` exists.** T4 was enforced BY THE TYPE, so the change is made loudly: the
   enum's doc keeps the old text verbatim beside what replaced it.
4. **`load_availability_reports` + `render_availability_reports`** — attributed claims, contest-
   marked, in a block the prompt explicitly separates from the adjudicated record.

**Why the routing subscription is correct now when it was judged wrong three days ago.** Both
objections in `2026-08-23_editor-to-scout-availability-plan.md` were consequences of the missing
`rating` slice, not of the route:

- *"Every injury packet that day is a SEPARATE enqueue."* With the key present, mig 225 mints
  `'pk:' || slice_fingerprints->>'rating'`, so the version IS the claim hash. Five outlets
  reporting one knock collapse to ONE enqueue. **The once-per-event-day rule falls out of CONTENT
  rather than a calendar key** — better, because it also holds across days when nothing new is
  said, and re-fires the moment a prognosis changes.
- *"It cannot carry the debounce bypass, so nothing runs."* Fixed by (2).

**T4, precisely.** Repealed: the Scout may read attributed claims. Still type-enforced: his slice
only — injury and suspension — no general packet prose, no headline framing, `sees_register`
false. No other voice's slice changes, and `facts` still carries no claims.

**The floor moved from the INPUT to the OUTPUT**, which is what "guards over evals" means here:
`mark_contested` still flags contradicting pairs mechanically and carries BOTH (T3/D6) — a
pointer, never a filter, because the disagreement is exactly what the Scout is being asked to
resolve.

**No `RATING_PROMPT_VERSION` bump.** s21 has asked for availability since 2026-08-22; what
changed is that the material exists to answer it. A bump folds into `input_components` and drains
a fleet the Articulator corpus is already queued behind.

---

## 7. Known gaps, stated rather than buried

**`current_season` = 2025 keys every Scout enqueue to last season.** Both
`enqueue_rating_for_applied_availability` and its transfer twin call it, and the `pk:` path
inherits it through `handle`'s fallback. An injury today mints a **2025** version and wakes the
Scout to rewrite last season's card. `stat_summaries` is 20,024 rows at 2025 and zero at 2026.
Rolling it to 2026 flips the fleet to a season with no stats, so the roll and the box-score rail
land together or not at all. **This is the top of the next session.**

**`mark_contested` is transfer-tuned.** Its negation list contains `"ruled"` (as in "ruled out of
contention"), so on injury prose *"ruled out"* reads as negated on BOTH sides and a genuine
contradiction goes unmarked. Both claims still reach the Scout attributed; he loses only the
pointer. Asserted in a test so it cannot regress silently. Widening the list changes the Insider's
marker too, so it was not done as a side effect.

**Mig 231 is validated but NOT applied.** It must ship with this Rust; applied early, the
fingerprint key is absent, the version falls back to the packet id, and the per-packet noise it
exists to prevent is live. Its dry run is what caught the column as `tag`, not `routing_tag`.

**`narratives` looks stalled** — 847 pending, nothing claimed since 09:50 on 2026-08-23, while
other stages drained normally the same evening. Noticed, not investigated.

---

## 8. Archbox, checked 2026-08-23 21:37

Not down. Up 28 days, load 2.7, psql answering, `scoracle-api` and `scoracle-cognition` both
active as **user** units (a system-scope `systemctl` listing does not show them — that misled this
session briefly). The queue claimed 21 items in five minutes.

The GPU is **saturated, not failed**: GTX 1070 Ti at 92% util, 134W/135W, 60°C, with
`llama-server` holding 4.8GB and `ministral-3:3b` resident. `cosmic-comp` and `Xwayland` share
GPU 0 with it, which is the likely reason a desktop session on that box feels frozen — starved,
not hung. `systemctl --user stop scoracle-cognition` releases the card; the queue is durable.

`dmesg` was not readable, so Xid faults were NOT ruled out.

---

## 9. Commits

| SHA | What |
|---|---|
| `59faed6` | availability record + the CHECK that would have eaten it (migs 228/229) |
| `42231f7` | seeder-era demolition; the fetch path goes honestly inert (mig 230) |
| `0340bf4` | schema snapshot after 228/229/230 |
| `c45fb6d` | retrieval through the budgeted fetcher; the registry is its address book |
| `a8354a0` | the availability record reaches the card |
| `c158c0c` | the Editor tags him and he judges the report himself |

433 lib tests pass, all targets build, clippy at its 12-warning baseline (all pre-existing).

Next steps: `planning_docs/PLAN-availability-and-boxscores.md`.

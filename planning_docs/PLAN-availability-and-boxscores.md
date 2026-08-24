# PLAN — availability tagging, and box scores from public sources

**Written 2026-08-23 (evening), at a deliberate pause.** Read
`progress_docs/2026-08-23_availability-tag-and-public-source-turn.md` first — it is the record of
what landed and why the shape changed. This file is only what comes NEXT.

Branch: `availability-record-and-public-source-turn`, six commits, **not pushed**.

**The governing ruling, Scott 2026-08-23** — apply it to everything below:

> *"I'd like to empower each model. Guards over evals. We can let the model do the work versus
> trying to engineer a rigid process."*

Read that as: put the code decision at the OUTPUT (a guard), not in upstream process
(adjudication tables, contract fields, status machines). `guards.rs`'s own mechanical-floor rule
already states the limit — a production guard earns its place for contract shape, integrity and
product leaks, **never** style or vocabulary taste, because in production the same check burns a
finished generation.

---

## Step 1 — `current_season`. Do this first; nothing else is honest until it lands.

`sports.current_season` = **2025** for all three sports while the season in play is 2026. Every
Scout enqueue keys on it — `enqueue_rating_for_applied_availability`, its transfer twin, and the
`pk:` packet path through `handle`'s fallback. **An injury today wakes the Scout to rewrite last
season's card.**

It is set MANUALLY; zero `UPDATE`s exist across `.rs`, `.go`, `.sql`, `.sh`.

**The trap:** rolling it to 2026 flips the whole fleet to a season with NO stats, because the
box-score rail has been dead since May. `stat_summaries` is 20,024 rows at 2025 and zero at 2026.
So the roll and the stats rail are coupled — **do not roll it alone.**

Also note the sports do not roll together: FOOTBALL's 2026/27 season is in play now, NFL starts
2026-09-09, NBA in October. A blind roll of all three is wrong.

Decision needed from Scott: automate the roll (off first completed fixture with real stats, per
sport?) or set it by hand per sport as each season's stats begin flowing, and file the
automation.

---

## Step 2 — ship the availability tag

Everything is built and tested; this is a deploy, not a build.

1. Deploy the branch's Rust to archbox (the `rating` slice must exist in
   `packets.slice_fingerprints` **before** the migration).
2. Apply `sql/migrations/231_scout_availability_routing.sql`. Already validated in a rolled-back
   transaction. **Order matters:** applied before the Rust, the fingerprint key is absent, the
   version falls back to the packet id, and the enqueue goes per-packet — noisy, not corrupting,
   but exactly what the slice exists to prevent.
3. Re-snapshot: `scripts/hosting/snapshot-schema.sh` (run `pg_dump` ON archbox — there is none on
   the Mac; see the progress doc).
4. **Watch the first real tags rather than assuming.** Useful checks:
   - `pipeline_work` rows at `stage='rating'` whose `input_version LIKE 'pk:%'`
   - `stat_summaries.trigger_type = 'availability'` appearing for the first time (mig 228 admits
     it; nothing has ever written it)
   - the collapse actually collapsing: several outlets on one knock ⇒ ONE row

**What to look for in the output, and where it belongs.** The Scout is now judging reports. Style
and register belong to the GATE's fixture expectations, not a production guard. Only promote a
check to `guards.rs` if it is a mechanical floor — e.g. a claim's number leaking into a tier or
rating, which the prompt forbids and which is string-checkable. Resist the urge to police tone.

---

## Step 3 — the `mark_contested` negation list

Known gap, asserted in a test so it cannot regress silently: the list is TRANSFER-tuned and
contains `"ruled"`, so on injury prose *"ruled out"* reads as negated on BOTH sides and a real
contradiction goes unmarked. Both claims still reach the Scout attributed; he loses the pointer.

Widening it changes the Insider's marker too, so it wants its own change with the Insider's
fixtures re-run — not a side effect of availability work.

---

## Step 4 — box scores: the source question, reopened honestly

**Established, so it does not get re-derived:** no open-licensed source carries current-season
per-player box scores for the five leagues. StatsBomb open-data is historical; openfootball is
CC0 but has no player data at all; football-data.org is a free TIER, not an open LICENCE, and
403s without a key. **OpenLigaDB (ODbL, keyless) is the only real candidate** and it is
crowdsourced, covers Bundesliga/LaLiga/PL only for 2026, and gives goalscorer-level detail — no
minutes, cards, or shots.

Scott, later the same session: *"We can find box scores somewhere else and I can drop that in."*
So treat the SOURCE as an input Scott supplies, and build the two halves that do not depend on
it:

**(a) The Investigator takes the HEADLINE.** Scott's design: *"the Editor notices a scoreline has
been reported → the Investigator is handed the headline so it knows what entities to search for →
box score is ingested."* `PacketDraft.headline` already exists (`best_headline`), and
`boxscore_sources.discovery` already has the `'search'` arm this session skipped in favour of URL
templates. This is strictly better than templates, because:

- a `{date}` template renders the ARTICLE's publish date, not kickoff (see the contamination
  finding) and would systematically miss;
- it does not depend on resolving one of the 174 article-derived fixtures;
- **it restores the reconciliation oracle** — the headline asserts a scoreline, the fetched page
  asserts a scoreline, and CODE rejects the ingest if they disagree.

**(b) The parser family + the reconciliation gate.** Build on the retained
`#[allow(dead_code)] // parser-family substrate (mig 230)` helpers in `boxscore.rs` — eleven
functions carrying Go-compatible number formatting. Do not rewrite them. The gate promotes
`trust_state` candidate → trusted.

Whatever source arrives, it still needs `terms_review->>'verdict' = 'pass'` before it is
eligible — that screen is enforced in code and is deliberate.

---

## Step 5 — the nomination pile (Scott chose this before the evening's turn; still open)

All 174 completed FOOTBALL 2026 fixtures are Editor nominations with `external_id IS NULL`, and
all 159 queued box-score rows target them. Chelsea "played 5 matches" on 2026-08-03; 35 team-days
are double-booked. `status='completed'` is currently a perfect synonym for "nominated,
unverified" — 289 rows, exactly the nomination count; real fixtures are `seeded`.

Two pieces, and the second is the one Scott named:

- **De-duplicate.** Prefer marking over deleting (the mig 208 "suspended, never deleted" habit,
  and `fixtures` is referenced by `event_box_scores`, `event_team_stats`, `notifications`,
  `fixture_boxscore_fetches`). A `superseded_by` column keeps the audit trail; a DELETE does not.
- **Stop them reaching the box-score queue uncorrected.** A `provisional` status is the obvious
  move (`fixtures.status` CHECK currently admits
  `scheduled|in_progress|completed|seeded|cancelled|postponed`, and `enqueue_fixture_boxscore`
  fires only on `completed|seeded`, so `provisional` would be inert for free).

**The tension to resolve first:** the box-score fetch is what `nominate.rs` expects to CORRECT
these rows (*"The Investigator's verified fetch revises start_time"*). If provisional fixtures
never enqueue, nothing corrects them. So decide what promotes provisional → completed before
making everything provisional, or the pile freezes instead of clearing.

Given the evening's ruling, the empowered shape is worth considering here too: hand the
Investigator the headline, let it find the real match, and let CODE reconcile the scoreline —
rather than building a status machine to gate what the model could resolve directly.

---

## Step 6 — only then, turn the stage on

`fixture_boxscore` is still absent from `COGNITION_STAGES` in `.env.local`, and that is still
correct. Turning it on before steps 4–5 drains 159 fixtures into `no_source`.

---

## Loose threads, not chased

- **`narratives` looks stalled** — 847 pending, nothing claimed since 09:50 on 2026-08-23 while
  other stages drained the same evening. Worth a look on its own.
- **No transfer has reached `applied` in ~4 weeks** (the handoff's §10). `is_rumor=f` is terminal
  and records no reason, so a completed move and a spurious co-mention are stored identically.
  60,475 `f` rows carry zero summaries; `NULL` is the retryable bucket, `f` is not. Rogers'
  Chelsea pairs are live and hot but vetted `f`. Its own session.
- **`dmesg` on archbox was not readable**, so GPU Xid faults were never ruled out. The card was
  merely saturated (92%, power-capped) at the time of checking, not failed.

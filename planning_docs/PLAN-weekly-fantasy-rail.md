# PLAN — The Weekly Fantasy Rail (rev 2, simplified)

Date: 2026-09-04. Status: DRAFT rev 2 — leanness pass folded in (Scott: "no extra roads").

The product, in one sentence: routinely run an RSS search for the teams in our DB, fetch
whatever event and entity data is missing, and tell the ongoing stories of the entities in
the news — weekly, memory-informed, through our characters.

The finished machine is three loops, all idempotent, all self-healing:

1. **The daily sweep** (one cron slot): RSS for our teams + schedule refresh + gap-fill
   stats fetch. No game → no gap → no work.
2. **The always-on drain**: the junctions telling stories, exactly as today. Untouched.
3. **The weekly seal** (one hourly Desk check): when a sport's week closes, each active
   seat writes its wrap-up.

Decisions locked:
- FOOTBALL scopes to the Premier League (FPL API); other four leagues freeze on 2025 stats.
- NFL = nflverse flat files. NBA = stats.nba.com. All three keyless and free.
- Weeks are true 7-day blocks; week 1 = each sport's opening day; 52-week round-the-year
  cycle; empty weeks render empty ("we present what we have"). One timezone: America/New_York.
- Fetch is **gap-driven, not Editor-triggered**: the feeds self-report completion (FPL
  `finished: true`, NBA scoreboard finals, nflverse week rows appearing), so the daily run
  asks "which finished fixtures have no stat rows?" and fetches exactly those. Event-driven
  without a detector; a failed fetch is just still in the gap tomorrow.
- The Investigator's *pattern* (work-driven gap-filling), not its machinery. Its fetch
  stack was built for hostile HTML; the feeds are structured JSON/CSV at fixed URLs. The
  news-side entity path (Editor → `investigate_entity` → metadata) stays exactly as is.
- Uniform card face: header, score nested top, body. Seats already write this (08-24 card
  contract) — display normalization, not a junction change.

---

## 0. Coverage audit — verified against live payloads 2026-09-04

The z-engine's entire raw-data contract is `rating_datapoints` / `rating_datapoints_team`
(`sql/schema/schema.sql:3578`, `:3730`).

### NFL — COVERED (verified against `stats_player_week_2025.csv` columns)
`https://github.com/nflverse/nflverse-data/releases/download/stats_player/stats_player_week_{season}.csv`
(also parquet; team twin under `stats_team`; schedules under `schedules`; rosters under
`rosters`/`players`). Every key the NFL arm reads exists — rename table at import:

| ours | nflverse |
|---|---|
| passing_yards / rushing_yards / receiving_yards | same |
| passing/rushing/receiving_touchdowns | passing_tds / rushing_tds / receiving_tds |
| passing_interceptions | passing_interceptions |
| fumbles_lost | fumbles_lost_total |
| kick_return_yards | kickoff_return_yards |
| punt_returner_return_yards | punt_return_yards |
| punt_yards | pt_yards |
| interception_yards | def_interception_yards |
| total_tackles | def_tackles_solo + def_tackle_assists |
| tackles_for_loss | def_tackles_for_loss |
| defensive_sacks | def_sacks |
| defensive_interceptions | def_interceptions |
| field_goals_made / extra_points_made | fg_made / pat_made |

`fantasy_points_ppr` arrives precomputed → stored verbatim; house `nfl.fantasy_points`
retires.

### NBA — COVERED
`pts reb ast stl blk fg3m plus_minus turnover pf fta` are standard boxscore fields.
Team arm's `pts_allowed` / `def_fg_pct` / `def_fg3_pct` derive from the opponent's row at
import.

### FOOTBALL — remap the arm to FPL-native vocabulary (mig 233)
FPL per-gameweek history (`/api/element-summary/{id}/`, `.history[]`) carries:
`goals_scored assists tackles clearances_blocks_interceptions recoveries
defensive_contribution saves goals_conceded clean_sheets own_goals penalties_saved
penalties_missed yellow_cards red_cards minutes starts bonus bps influence creativity
threat ict_index expected_goals expected_assists expected_goal_involvements
expected_goals_conceded total_points round fixture opponent_team was_home team_h_score
team_a_score`.

Only Goalscoring, Creation, Tackling (drop the `team_opp_possession` normalizer), and the
GK saves labels survive directly; shots on target, accurate passes, key passes, dribbles,
possession lost, and split clearances/blocks/interceptions do not exist in FPL. So the
FOOTBALL arms (player + team) are rewritten to FPL's own axes — richer than what we lose:

- Outfield: Goalscoring, Creation (assists), xG, xA, Threat, Craft (creativity),
  Influence, Defensive Work (defensive_contribution / tackles+CBI+recoveries),
  Discipline (cards, sign −1), Bonus Merit (bps).
- GK: Shot-Stopping (saves), Goals Prevented (saves vs `expected_goals_conceded` — better
  than the league-avg proxy), Clean Sheets.
- Team: aggregated from player rows + fixture scores — Goals For/Against, Clean Sheets,
  xG For/Against, Defensive Work, Cards, Bonus.

`rating_thresholds` FOOTBALL rows re-key on `minutes`. FPL `total_points` stored verbatim
as fantasy points; house `football.fantasy_points` retires. Non-PL leagues: 2025
`player_stats` rows freeze as-is; stored JSONB breakdowns stay readable without
recomputation.

---

## 1. Phase A — the data step in the daily sweep

One new step in the existing `go/cmd/pipeline` binary, same advisory-lock + `pipeline_runs`
pattern, same 02:00 cron slot as RSS (order: schedules → RSS → stats gap-fill). No new
daemon, no new stage, no Editor coupling. Per run:

1. **Schedule + roster refresh.** Three fixed sources (nflverse `schedules` + rosters;
   NBA schedule/player index; FPL `bootstrap-static` + `/api/fixtures/`). Upsert
   `fixtures` (with `external_id`), upsert players/teams + `import_identity(source,
   external_id, entity_type, entity_id, sport)` — the minimal successor to the demolished
   provider maps (FPL ships `opta_code`; nflverse ships ID crosswalks). **Guard: roster
   upserts never enqueue voice work** — entity existence comes from data; entity stories
   stay news/storyline-driven (preserves the storyline-placed inflow discipline).
   `sports.current_season` and `season_weeks` (§2) maintained here — automation replaces
   the manual pin.
2. **Gap query.** `fixtures` finished per the feed, with no `event_box_scores` rows.
3. **Gap fill.** For each gapped fixture: fetch (NFL: the covering week file, deduped per
   week; NBA: per-game boxscore; FPL: fixture/element-summary rows), rename keys per §0,
   resolve identity via `import_identity`, then in one transaction write
   `event_box_scores` + `event_team_stats` and call `finalize_fixture(fixture_id)` —
   its designed caller at last (`schema.sql:2699`). `recompute_season` once per
   (sport, season) per run. Unresolvable rows → funnel counters (RSS-funnel doctrine),
   retried next run by construction.

Self-healing by shape: a missed day, a source outage, a half-run — all just leave gaps the
next run closes. Watchdog gains `stats_recency` per sport, armed only for weeks where
`season_weeks` has fixtures (an empty offseason week must not alarm).

Downstream, for free: `compute_event_starline` produces per-event ratings again → Scout
trajectory un-starves → momentum's rating slope refills; `cron-stat-matchups.sh` revives
untouched.

### A-demolition (the pruning ledger — what this build deletes)
- The inert box-score substrate: `boxscore.rs` parser-family helpers and fetch path,
  `boxscore_sources`, `fixture_boxscore_fetches`, `enqueue_fixture_boxscore` + its
  trigger, the `fixture_boxscore` stage enum entry.
- Editor fixture nomination (`nominate.rs` fixture creation) and the whole
  `needs_verification` flow — schedules are authoritative; the 174 unverified FOOTBALL
  fixtures reconcile against the FPL fixture list, unmatched ones tombstoned. The Editor
  sheds a responsibility. (`investigate_entity` for news-side entities stays.)
- House `nfl.fantasy_points` and `football.fantasy_points` (imported actuals).
- Jan-1 week arithmetic, backend (`/headlines` CTE) and frontend (`week.ts`).
- Rolling scope-window SQL variants (re-keyed to the week grid, §2).
- Manual `sports.current_season`.
- The ET/UTC/server-local day-boundary patchwork (everything keys to `season_weeks` in ET).

---

## 2. Phase B — the 52-week cycle

### B1. `season_weeks` (mig 236)
`(sport, season, week_no 1..53, starts_at, ends_at, PRIMARY KEY (sport, season, week_no))`
— generated by the schedule refresh: week 1 opens 00:00 ET on the sport's opening day;
strict 7-day blocks; rows run round-the-year until the next season re-anchors. Helper
`week_of(sport, ts)` for bucketing and backfill.

### B2. Week keys (mig 237)
Add `season, week_no` (nullable) to the eight card tables (`news_summaries`,
`vibe_scores`, `insider_scores`, `transfer_rumors`, `momentum_summaries`,
`stat_summaries`, `sigil_synthesis`, `oracle_readings`), stamped at insert, backfilled via
`week_of(generated_at)` so all history is browsable immediately. Also stamp
`snapshot_rating_history` rows (powers Profile evolution, §3). `generated_at` stays the
generation id; the week is the shelf it files on.

### B3. Posture + seal
Junctions keep their event-driven shape. Changes:
- **In-week regens are revisions**: each seat's prompt gets the week frame (reads its own
  latest generation this week + memories; "the day's" → "the week so far" in
  `journalist/prompt.rs`, `influencer/prompt.rs`, cycle language in `junctions/mod.rs`).
  Week window derived in code, handed to the model (directing doctrine).
- **The seal**: hourly Desk check in `worker.rs`; when a sport crosses
  `season_weeks.ends_at`, re-enqueue once (with a `sealing` flag → wrap-up posture) every
  seat that generated that week, stamped into the closing week. Idempotence via
  `week_seals(sport, season, week_no, sealed_at)`. Empty weeks seal as no-ops.
- **Cadence-semantics re-base**: `cron-narrative-links.sh` trajectory becomes
  delta-vs-prior-*week's*-sealed-strength, not delta-vs-previous-run (the comment says it
  itself: "the cadence IS the trajectory baseline"). `trajectory::classify_delta` is
  prior-state-anchored already — untouched.

### B4. Momentum, weekly (mig 238)
`refresh_momentum_scores`: vibe slope = last 3 sealed weeks, week-over-week (same span as
the rolling 21 days, aligned to the grid). Rating slope = avg per-event rating per week,
last N weeks (`season_bridge_window` restated: NBA 3 / NFL 2 / FOOTBALL 4); weeks with no
events drop out of the window rather than zeroing it (the mig-130 bye-week lesson, kept).
Snapshot thinning: per-week grain after 30 days. Analyst unchanged beyond the week frame.

### B5. API
- `GET /{sport}/weeks?season=` — the grid + which weeks each seat has content
  (one cheap aggregate; powers nav).
- Every card endpoint gains optional `?season=&week=`: omitted → live current week
  (current behavior); present → latest generation stamped in that week, i.e. the sealed
  wrap-up for closed weeks. `/headlines` switches to the same `season_weeks` join.
- `scope=current_week` etc. become sugar for real week rows; labels stay.
- Vibe's fixed 7-day snapshot window aligns to the grid.
- Sealed weeks are immutable → long edge-cache headers.

---

## 3. Phase C — cards (polish tier)

- **Profile card**: the pizza chart, week-scoped (evolution = the week's last
  `snapshot_rating_history` row). `ScoutingCard` drops the `view=chart` flip, goes
  prose-only. Registry touchpoints are compiler-enforced (`ProfileTab` union,
  `card-registry.tsx`, `characterName`, `DECK_HUES`, tarot deck, `deck-content.ts`
  presence, `deck-scores.ts`, `VALID_TABS`, motif SVG + token, fetcher reusing `getStats`
  with `week`, preload warm). EntityMeta ring: Profile stays off it — no re-layout.
- **Uniform card face**: header (headline), score nested top (`CardScoreSlot`), body.
  Per-seat flourish moves inside the body; the frame is one template. WeekCard's archive
  face becomes the sealed-week card face.
- Frontend week nav: `week.ts` → `/weeks`; NavRail Select goes sport-aware; "Today" =
  live current week. One flip of the scope = any week, any card.

---

## 4. Sequencing

1. **NFL slice of Phase A** (in-season now, cleanest feed): schedules + rosters +
   identity + gap-fill → prove the loop end-to-end (ratings move, trajectory
   un-starves).
   **BUILT 2026-09-04** (not yet applied/deployed): mig 233 (identity conflict target
   on `entity_external_ids` — reused, no new table; `players.id` sequence; dead
   boxscore-enqueue trigger dropped), `go/internal/dataimport` (gap-driven importer),
   `pipeline -mode data`, cron line 01:30. Dry-run vs prod: 32/32 teams match with an
   `LA→LAR`/`WAS→WSH` adapter dialect map; roster names 2,886/3,133 unique match,
   200 creates, 47 ambiguous (team-narrowed). First run: apply mig 233 → deploy →
   `pipeline -mode data -season 2025` (full-season backfill proof) → install crontab
   lines.
2. **NBA** (opens ~Oct) and **FPL + mig 233 remap**.
3. **A-demolition** once the rail carries all three.
4. **B1–B2** (grid + stamping + backfill) — pure additive.
5. **B3–B5** — the cycle flip.
6. **Phase C** — the final polish tier, then done.

## 5. Open items (deliberately short)
- FPL arm final label set + in_comp/facet assignments (taste call at mig 233).
- Non-PL leagues: "last season" posture on their frozen cards.
- nflverse in-season latency: confirm week-1 live timing before trusting the 02:00 slot
  (assets typically update within hours of games; worst case the gap heals next run).

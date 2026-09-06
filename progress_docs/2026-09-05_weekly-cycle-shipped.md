# 2026-09-04/05 — The weekly cycle ships

Two days, ten migrations (233–242), five backend releases, four frontend
deploys. The product's final shape (PLAN-weekly-fantasy-rail.md rev 2 + the
§2b scope collapse) went from plan to production. Everything below is LIVE.

## What shipped, in dependency order

1. **The data rail** (mig 233, `go/internal/dataimport`, `pipeline -mode data`,
   crontab 01:30). Gap-driven: schedules + rosters from nflverse, then every
   feed-finished fixture with no stat rows promotes through `finalize_fixture`
   — its first-ever caller. The feed detects the event; a miss is still in the
   gap tomorrow. 2025 was already vendor-seeded, so the first run proved
   IDEMPOTENCE instead (adopted + bound 557 fixtures across 2025/2026, created
   201 players at a 92% roster name-match). The dead box-score enqueue trigger
   fell with it. `sports.current_season` rolls itself now.

2. **The person layers reconciled** (mig 234) + **dynamic entity metadata**
   (migs 235/236). narrative_persons ↔ persons bridge nightly (first crank:
   123 linked, 228 nominated with evidence); 1,466 verified persons went live
   in /api/v1/entities; coaches are transfer-eligible (subject_type in every
   pair key; t11-person prompt variant, player path byte-identical);
   entity_fact_policy is the write-permission table (ABSENT = FROZEN — a
   team's city is untouchable by any model path) and refresh_dynamic_entities
   is the nightly clock (news-active + >30d stale, 25/class/sport).

3. **The calendar** (migs 237–240, three verification-driven corrections in
   one evening — each caught in prod minutes after apply):
   - 237: season_weeks + week stamps on 10 tables (backfilled) + week_seals.
   - 238: opening day is the SCHEDULE's word (bound fixtures ≥30), not the
     news's memory of preseason. (Also: the DELETE-in-CTE snapshot trap.)
   - 239: an unbound season needs ≥100 fixtures — summer noise can't mint a
     season; NBA truthfully reads its offseason tail. restamp_card_weeks()
     became a durable function (the grid moves again at every rail landing).
   - 240: every week turns Monday 00:00 ET (Scott's boundary decision); week 1
     = the Monday week containing opening day. 1.69M stamps re-laid.

4. **The weekly surface** (B5 + B3 + B4): GET /{sport}/weeks (the nav's
   calendar; two newest seasons); /headlines re-keyed to reporting weeks; the
   seal — closing pass in the week's final 6h (seal:SEASON-WK input_version,
   junction content-debounce keeps it cheap), boundary stamps sealed_at
   (2,031 historical weeks sealed on first boot); momentum re-windowed onto
   week-state averages (momentum_week_window: NBA 3 / NFL 2 / FOOTBALL 4;
   empty weeks drop out — the mig 130 bye rule kept).

5. **The cards** (frontend): the Scouting/Profile split — Profile is the
   Scout's chart with every per-x scope + compare; Scouting is prose with NO
   controls (the rail's year+week axis is its frame); one shared composite
   score so the two faces can never disagree. The uniform prose face:
   tweet-sized hook + GemmaSummary's balanced ≤3-sentence paragraphs (legacy
   run-ons split at render — every past generation reads cleanly without
   regeneration). Momentum's season control retired.

6. **The boards**: year+week on news/transfers/vibes/sigil boards AND the
   per-entity /news + /transfers products (archives don't age; unknown weeks
   are empty, never fallbacks); rate=per_x ranks the rating board wholesale
   from rating_modes. Frontend week dropdown shared with the profile rail
   (?week=SEASON-N), Rate select for players.

## Bugs found and fixed along the way

- **Momentum text face dark since August**: the mig-226 card contract renamed
  the summary payload keys to heat/body; the frontend read score/blurb —
  verdict.blurb was always undefined, so real verdicts rendered as "reading
  pending". Type + readers fixed.
- **Eval fixture generators broken**: LazyLock prompt refactor vs json! in
  three examples (momentum_s6 / rating_s14 / oracle_or4). The refreeze path
  builds again — the posture tuning session depends on it.
- Calendar corrections above (237→240) — each an honest prod-verification
  catch, none reached users (frontend deployed after 240).

## The weekend runs itself

- Sun 2026-09-06 ~18:00 ET: first closing pass, all three sports.
- Mon 2026-09-07 00:00 ET: first seal AND NFL 2026 week 1 opens — same
  midnight.
- Thu 2026-09-10: the opener's box score is the promotion path's first live
  game (watch logs/pipeline-data.log the morning after: gaps_filled > 0,
  ratings move, the Scout's trajectory un-starves for 2026).

## Standing watch items

- investigate_entity backlog ~7k, drains ~900/day (Wikimedia 2s politeness is
  the ceiling — not GPUs); the reconcile/refresh clocks add pre-gated trickle.
- 54 unbound "completed" 2026 NFL fixtures (news-era preseason nominations) —
  the deferred tombstone class, inert since the trigger drop.
- Fixture field-updates converge over the first nightly runs (counter noise).
- nflverse in-season latency unproven until week 1 (worst case: the gap holds
  a game one extra day).
- WATCHDOG_ALERT_URL still unset (standing item, predates this arc).

## Next sessions, in order

1. Weekly prompt posture — "the week so far" framing for the Journalist +
   Influencer; prompt-version bumps + eval refreeze; SUPERVISED (voice).
2. NBA rail at season start (~Oct): stats.nba.com adapter on the same gap
   query; bindings re-anchor the NBA calendar exactly.
3. FPL rail + the FOOTBALL z-arm remap (the original mig-233 plan §0) —
   unlocks the fantasy-scope retirement and the A-demolition.

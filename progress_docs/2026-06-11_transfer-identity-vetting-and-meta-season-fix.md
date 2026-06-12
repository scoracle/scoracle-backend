# 2026-06-11 — Transfer identity vetting + meta stale-club fix

Session handoff. Two related workstreams, both **planned not yet coded**. Pick up on Archbox.

## ⚠️ FIRST THING ON ARCHBOX: `git fetch && git pull --ff-only`
This session was nearly wasted because I started searching a ~2-week-stale `main` and
concluded the transfer engine "didn't exist" — it was on `origin/main` the whole time
(per the CLAUDE.md "sync is ALWAYS step 1" rule, which I skipped). Don't repeat that.

---

## Goal
Sharpen the **Transfers · Heat** engine: the leaderboard shows the same player multiple
times and attributes coverage to the wrong entity. Examples from prod
(`scoracle.com/profile?sport=FOOTBALL&type=team&id=18`, Chelsea):
- "Nícolas" (#2) ranked on a summary that's clearly about **Nicolas Jackson** (wrong entity).
- "João Pedro" appears **twice** (#5, #7) + a bare "Pedro" (#8) on the same coverage.
- (Atlético) "Bernardo" served on **Bernardo Silva** coverage.
- "Florentino" (player Florentino Luís, id 163210) catches **Florentino Pérez** (Real
  Madrid president) mentions.

---

## Workstream 1 — Transfer identity vetting (Task #1)

### Architecture (as-is)
`go/internal/ml/transfer.go` is **pair-grained**: for a team it finds co-mentioned
players (`loadCandidates`, ~line 254), computes deterministic heat in SQL
(`compute_transfer_heat`, migration 032), and calls Gemma **once per (team, player) pair**
to vet it (`analyzePair`, ~line 121). Gemma returns
`{is_rumor, direction, stage, summary, confidence}`. Rows with `is_rumor=false` are
**already filtered out** by the read (`db.go` transfer leaderboard stmt, `WHERE is_rumor IS TRUE`).

### Root cause
The candidate seed (`loadCandidates`) joins `news_article_entities`, which is populated by
the **loose lexical matcher** in `go/internal/thirdparty/match.go` (substring + first/last-name
word-boundary). Every name collision becomes a spurious (team, player) candidate. Gemma is
then asked the wrong question — "is this a rumor about <name>?" — and never "is this the
**same** person?", so it confirms name-collision noise.

### The fix (lean — agreed direction)
Don't add a pass or a matcher. **Sharpen the one question Gemma already answers** and let the
existing `is_rumor=false` discard path do the work. Key insight: **discard is lossless** — the
wide net also links the article to the *correct* entity, so that pair is already in the
candidate set; dropping the impostor loses nothing (so NO need to reroute to the right entity).

Concrete changes, all in `ml/transfer.go`, **no migration, capture untouched**:
1. **Enrich `loadCandidates`** to also select `nationality`, **current club**, `position`.
   ⚠️ **Current club MUST come from the `player_stats` MAX(season) row, NOT `players.team_id`**
   (that field is stale — see Workstream 2). Mirror the leaderboard's expression.
2. **Identity card in `buildTransferPrompt`** — one line, led by current club, e.g.
   `João Pedro · Brazil · currently at Brighton · FW`.
3. **Rewrite `transferSystemPrompt`** so `is_rumor=true` requires it's **THIS exact player**
   AND a real transfer. Add a name-collision warning with the president example. Identity
   folds into `is_rumor` — no new column needed.
4. **Have Gemma also return `subject`** (who the sources are actually about) and stash it in
   the existing `trigger_payload` JSONB for an audit trail of what got discarded. No migration.

### Explicitly CUT (avoid over-engineering)
- ❌ Deterministic name-token backstop — identity is an easy LLM judgment (unlike the subtle
  former-player gate that needed Go). Add ONLY if QA shows leakage.
- ❌ Rerouting to the correct entity (discard is lossless).
- ❌ `match.go` / capture changes — the wide net is a feature.
- ❌ Schema migration.

### Flagged, not solved
If the two "João Pedro" rows are **duplicate DB entities for the same human** (vs two real
players), no per-pair check splits them — that's a separate entity-dedup task.

### Pre-flight (BLOCKER before coding — needs prod DB; I had no creds on the other machine)
Confirms the identity card has signal. Run read-only on prod:
`psql "$PROD_READ_URL" -f /tmp/transfer_preflight.sql` (SQL inlined below — `/tmp` copy did
not travel).

```sql
-- FOOTBALL identity-card pre-flight (read-only).

-- 1. Nationality coverage (does the card have signal?)
SELECT count(*) AS players, count(nationality) AS has_nationality,
       round(100.0*count(nationality)/nullif(count(*),0),1) AS pct_nationality
FROM players WHERE sport='FOOTBALL';

-- 2. Current club resolvable from player_stats MAX(season)? + how stale is players.team_id
SELECT count(DISTINCT p.id) AS players,
       count(DISTINCT p.id) FILTER (WHERE cur.team_id IS NOT NULL) AS has_current_club,
       count(DISTINCT p.id) FILTER (WHERE p.team_id IS DISTINCT FROM cur.team_id) AS stale_players_team_id
FROM players p
LEFT JOIN LATERAL (
    SELECT ps.team_id FROM player_stats ps
    WHERE ps.player_id=p.id AND ps.sport=p.sport AND ps.team_id IS NOT NULL
    ORDER BY ps.season DESC LIMIT 1
) cur ON true
WHERE p.sport='FOOTBALL';

-- 3. Spot check the names that broke: stale vs current club + disambiguators
SELECT p.id, p.name, p.nationality,
       st.name AS stale_club,    -- players.team_id (meta bug)
       ct.name AS current_club    -- player_stats MAX(season) (correct)
FROM players p
LEFT JOIN teams st ON st.id=p.team_id AND st.sport=p.sport
LEFT JOIN LATERAL (
    SELECT ps.team_id FROM player_stats ps
    WHERE ps.player_id=p.id AND ps.sport=p.sport AND ps.team_id IS NOT NULL
    ORDER BY ps.season DESC LIMIT 1
) cur ON true
LEFT JOIN teams ct ON ct.id=cur.team_id AND ct.sport=p.sport
WHERE p.sport='FOOTBALL' AND p.name ILIKE ANY (ARRAY[
    '%olise%','%pulisic%','%bellingham%','%joão pedro%','%joao pedro%','%pedro%',
    '%nicolas%','%nícolas%','%bernardo%','%florentino%'])
ORDER BY p.name;
```
Reading: Q1 high `pct_nationality` → strong card; low → lean on current club + position.
Q2 `has_current_club` ≈ all players; `stale_players_team_id` also sizes the meta bug. Q3 is
the money table — expect stale≠current for Olise/Pulisic/Bellingham, and shows whether the
collision entities (Nícolas, 2nd João Pedro, Florentino) have distinguishing nationality/club.

---

## Workstream 2 — Meta/search shows stale (most-recently-SEEDED) club (Task #2)

### Symptom
Search autofill + profile dropdown show a player's club from their most-recently-**seeded**
season, not most-recently-**played**. Pulisic→CHE (should be AC Milan), Bellingham→BVB
(should be Real Madrid), Olise→Crystal Palace in dropdown but **Bayern (correct) on the
rating leaderboard**.

### Root cause (confirmed in code)
- **Wrong:** `sql/football.sql` `football.autofill_entities` materialized view (~616-662)
  selects `p.team_id` and joins `teams ON t.id = p.team_id`.
- **Why stale:** `seed/shared/upsert.py` `upsert_player()` (~line 98) does
  `team_id = COALESCE(EXCLUDED.team_id, players.team_id)` — backfilling an OLD season stamps
  `players.team_id` with that season's club. (`player_stats.team_id` is correctly per-season.)
- **Correct reference impl:** `go/internal/db/db.go` leaderboard stmt (~329-426) joins
  `teams ON t.id = player_stats.team_id` filtered to the player's season.

### Fix
In `autofill_entities`: resolve club from the MAX(season) `player_stats` row — select
`ps.team_id`, join `teams ON t.id = ps.team_id` (reorder joins; keep
`DISTINCT ON (p.id) ... ORDER BY p.id, ps.season DESC NULLS LAST`). Then **REFRESH the
materialized view**. (Apply migration before restarting the Go API per CLAUDE.md — `db.New`
prepares statements at boot.)

### Shared gotcha
This is the **same stale-field trap** as the transfer identity card. Both must derive current
club from `player_stats` MAX(season). Defining one reusable SQL expression/CTE would keep them
from drifting.

---

## Task tracker
- **#1** Transfer identity vetting — blockedBy #2 (shares the current-club expression; not a
  hard ship-block). Gated on the pre-flight.
- **#2** Meta stale-club fix.
Suggested order: pre-flight → #2 (also unblocks the shared expression) → #1.

## Key files
- `go/internal/ml/transfer.go` — engine (loadCandidates, analyzePair, transferSystemPrompt, buildTransferPrompt)
- `go/internal/thirdparty/match.go` — loose lexical matcher (root of the noise; leaving as-is)
- `go/internal/db/db.go` — transfer leaderboard read (`is_rumor IS TRUE`) + correct club ref impl
- `sql/football.sql` ~616-662 — `autofill_entities` (the meta bug)
- `seed/shared/upsert.py` ~98 — where `players.team_id` goes stale
- `sql/migrations/031_transfer_rumors.sql`, `032_transfer_heat.sql` — transfer schema + heat

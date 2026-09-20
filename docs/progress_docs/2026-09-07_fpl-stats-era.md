# 2026-09-07 — The FPL stats era: full payload, dual-era z-model, native pizzas

One day, six migrations (244–249), two importer fix waves, two identity merges.
The football stats rail stopped speaking the vendor's language and started
speaking FPL's — without breaking a single archived season. Everything below is
LIVE (backend `80808c6`).

## The arc

Three matchweeks into the PL season the rail was promoting fixtures but the
season rows aggregated to empty shells and every 2026 rating was NULL. Scott's
brief: backfill the missed games, rework the z-model for the new format, use
the FULL fantasy payload, and curate what gets served. By end of day: 15/15
FPL-finished fixtures promoted with the full payload, 146/295 players rated
(Bruno Fernandes on top at a 92.9 score), 18/18 teams rated, and the Profile
pizza rendering FPL-native wedges — with zero frontend code changes.

## What shipped, in dependency order

1. **The curated vocabulary** (mig 244). Fifteen `stat_definitions` rows are
   the FPL-era curation surface: xG/xA/xGI/xGC, defensive_contribution, CBI,
   clean sheets, bonus points, ICT served; bps/influence/creativity/threat
   stored but never graded. Flipping a row between served and internal is an
   UPDATE, not a deploy — the percentile machinery is key-agnostic and adapts
   through the definitions.

2. **Key-agnostic season aggregation** (migs 245–246). The vendor-era
   aggregators hand-enumerated their vocabulary and gated on a column the FPL
   importer never wrote, so 2026 season rows were `{"fantasy_points": 0.00}`.
   Both `football.aggregate_*_season` functions now sum whatever numeric keys
   the events carry (percent-shaped keys average; per_90/per_game derive only
   where an `is_derived` definition exists). **The hard-won rule: whole numbers
   must emit as jsonb integers.** `idx_team_stats_points` evaluates
   `(stats->>'points')::integer` at insert time and `"3.00"` does not cast —
   ROUND(,2) on a count killed `finalize_fixture` for every fixture.

3. **The dual-era z-model** (mig 247). The FOOTBALL arm of `rating_datapoints`
   spoke pure vendor keys (shots_on_target, passes_accurate, key_passes) —
   almost none of which FPL provides. Rebuilt dual-era: each label
   COALESCEs vendor key → FPL key, and since a season's rows are
   era-homogeneous, every cohort resolves one branch and the z-distributions
   stay internally consistent. New labels: Chance Creation (xA), Defensive
   Work (FPL's DC composite), CBI, Discipline (cards — captured in both eras,
   never rated), GK Clean Sheets, and Goals Prevented = xGC − conceded. Teams
   gained xG For/Against and Clean Sheets. Three truth rules, each measured
   into existence:
   - **Zero-fill counting stats at the rating layer.** FPL zero-suppresses
     (a 0-goal defender has no `goals` key), and absence z-scores as "at the
     mean" — non-scorers were ranking mid-table in Goalscoring. Season rows
     stay sparse; only the z-model densifies.
   - **Drop era-dead labels** (pop mean IS NULL) in the z step. An all-tie
     label percent_ranks everyone to 0 and reads as a fabricated liability on
     the Scout's card. Player breakdowns went from 18 labels (5 dead) to 13.
   - **FPL-era-only zero-suppressed keys 0-fill behind an era-shape guard**
     (`p_stats ? 'expected_goals_conceded'` …) so the dead-label filter
     survives vendor recomputes.
   The appearance gate **self-scales**: `LEAST(configured 10, ceil(half the
   cohort max))` — 3 matchweeks in, 2 appearances ranks you; converges to the
   configured 10 by ~matchweek 20. Generic, so NFL/NBA early seasons also
   ignite sooner on their next recompute.

4. **FPL-native pizzas** (mig 248). The Profile card's Regular-mode wedges come
   from `stat_templates`, which carried only vendor keys — a 2026 pizza would
   have been all zeros. Templates now carry a `variant` ('vendor'|'fpl') and
   the template functions pick by ROW SHAPE, so each season's pizza is native
   to its own data and the 2019–2025 archive renders untouched. Also:
   `position_group` learned FPL's "Forward" (the vendor said "Attacker") —
   every FPL forward had silently lost their template.

5. **Progression emits absence** (mig 249). A `COALESCE(…,0)+COALESCE(…,0)`
   datapoint expression defeats the dead-label filter — dense zeros have a
   mean — so the team card drew a "Progression 0" wedge on every club. Rule:
   era-absent sources emit NULL (presence/era-shape guard), never 0.

6. **The matcher grew two rungs** (fpl.go). Found via two duplicate rows the
   importer minted: (a) the "first+last token" rung called `splitName`, which
   returns first + REST — it re-queried the full name verbatim and never cut a
   middle name, so "Bruno Borges Fernandes" sailed past house "Bruno
   Fernandes"; (b) FPL web names are initialed ("A.Becker"), so single-token
   house names ("Alisson") were unreachable by any rung. Fixed with a true
   first+last-token rung and a team-scoped mononym rung. The duplicates were
   **merged onto their canonical ids** (129602, 129820 — events, season rows,
   FPL bindings, surfaces moved; provenance in `meta.merged_duplicate`), so
   `available_seasons` spans 2019–2026 again.

## Operational lessons

- **SQL functions inline into the Go API's prepared statements.** Replacing a
  function body does not reach the API until `systemctl --user restart
  scoracle-api` — Bruno served vendor wedges from the cached plan while direct
  SQL served FPL ones. This belongs beside "migrations before binaries."
- **Record the ledger when applying by hand.** `schema_migrations` was 15
  records behind (233–247 all applied via direct `psql -f`); a future
  `migrate.sh` would have replayed the lot. Backfilled; migrations 248+
  self-record inside their own transaction.

## The reverse-seed decision

Should past seasons be reverse-seeded from fantasy data? **No.** The house
holds seven full vendor seasons (2019–2025, ~3,400 players each); FPL history
covers only current PL players and only as season totals, so a partial reseed
would mix vocabularies inside a season and break the era homogeneity the
dual-era model depends on — and could never rebuild the event layer. The
continuity that matters (Scout movement lines compare percentiles per label)
already bridges on the shared spine. The one open offer: a deliberate
recompute of FOOTBALL 2025 under the dual-era model would align the label set
completely (Key Passes → Chance Creation, Discipline gains a baseline) at the
cost of slightly shifting a concluded season's public composites. Frozen until
Scott says go.

## Standing state

- 58 house-finished fixtures fill as FPL's own finished-flag catches up (the
  4×/day sweep owns this; by design, not a gap).
- 7 ambiguous players (the "Costinha" class) stay fail-closed.
- The mig-244 curation split awaits any pruning Scott wants — one UPDATE each.

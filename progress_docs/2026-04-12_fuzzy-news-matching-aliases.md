# Fuzzy News Matching via Search Aliases

**Date:** 2026-04-12

## Problem

Teams like FC Bayern München have many name variants in English-language news ("Bayern Munich", "FC Bayern", etc.). The news service searched and filtered by the single DB name only, causing missed articles.

## Solution

Added a `search_aliases TEXT[]` column to teams and players, with an automatic alias generation pipeline.

### Alias Generation (`seed/shared/aliases.py`)

- Strips common club prefixes (FC, SC, AC, VfL, etc.)
- Transliterates diacritics (ü→u, ö→o, etc.)
- Includes short_code and full_name from meta
- Manual override dict for known problem cases (Bayern, Gladbach, Atlético, etc.)
- Auto-runs during `upsert_team()` / `upsert_player()` if aliases aren't pre-set

### Go News Service Changes

- **Multi-variant RSS search**: if primary name finds < 3 articles, tries the best alias as a fallback query
- **Alias-aware filtering**: `nameInText()` checks all aliases when filtering articles
- **False-positive guard**: short aliases (<4 chars) only match if sport-context terms appear in the text
- New prepared statements: `team_news_lookup`, `player_news_lookup` (include `search_aliases`)

### Database

- Migration: `sql/migrations/001_add_search_aliases.sql`
- GIN index on `teams.search_aliases`

## Decisions

- **No co-mentions** — frontend handles this (has the entity DB, guarantees matching results)
- **Aliases live in meta/profile seeding path** — event seeding focuses on raw box score payloads
- **Overrides use both diacritic forms as keys** — matches regardless of how the provider spells the name

## Files Changed

| File | Change |
|------|--------|
| `sql/shared.sql` | `search_aliases TEXT[]` on teams + players |
| `sql/migrations/001_add_search_aliases.sql` | Migration for existing data |
| `seed/shared/aliases.py` | Alias generation logic |
| `seed/shared/models.py` | `search_aliases` field on Team + Player |
| `seed/shared/upsert.py` | Auto-generate aliases during upsert |
| `go/internal/db/db.go` | New prepared statements |
| `go/internal/api/handler/news.go` | Lookups return aliases |
| `go/internal/thirdparty/news.go` | Multi-query search + alias filtering |
| `go/internal/thirdparty/news_test.go` | 9 unit tests |

## Seeding Status

All teams seeded with aliases (teams-only pass, players capped at 1):
- Bundesliga (18), Premier League (20), La Liga (20), Serie A (20), Ligue 1 (18)
- NBA (30), NFL (32)
- Full player alias seeding pending next `scoracle-seed meta seed` run

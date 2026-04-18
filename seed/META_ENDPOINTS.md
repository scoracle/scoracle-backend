# Metadata Endpoints

This document describes the profile/metadata endpoints available for each sport.

## Overview

The metadata system is separate from event seeding and provides:
- Player profiles (photos, nationality, height, weight, DOB)
- Team profiles (logos, venues, founding info)
- Jersey numbers and positions

This separation allows provider-agnostic event data and flexible metadata sourcing.

---

## NBA (BallDontLie API)

**Base URL:** `https://api.balldontlie.io`

### Player Profile

**Endpoint:** `GET /v1/players/{id}`

**Example:** `https://api.balldontlie.io/v1/players/237`

**Response Fields:**
- `id` - BDL player ID
- `first_name` - First name
- `last_name` - Last name  
- `position` - Position (G, F, C, etc.)
- `height` - Height (e.g., "6-9")
- `weight` - Weight in lbs
- `jersey_number` - Jersey number
- `college` - College/high school
- `country` - Country of origin
- `draft_year` - Draft year
- `draft_round` - Draft round
- `draft_number` - Overall pick number
- `team` - Current team info

### Team Profile

**Endpoint:** `GET /v1/teams`

**Fields:** id, name, city, conference, division, abbreviation

### All Players

**Endpoint:** `GET /v1/players`

**Query Parameters:** per_page (max 100), page

---

## NFL (BallDontLie API)

**Base URL:** `https://api.balldontlie.io`

### Player Profile

**Endpoint:** `GET /nfl/v1/players/{id}`

**Example:** `https://api.balldontlie.io/nfl/v1/players/1`

**Response Fields:**
- `id` - BDL player ID
- `name` - Full name
- `position` - Position (QB, RB, WR, etc.)
- `jersey_number` - Jersey number
- `college` - College attended
- `country` - Country of origin
- `team` - Current team

### All Players

**Endpoint:** `GET /nfl/v1/players`

**Query Parameters:** season (required), per_page (max 100)

---

## Football/Soccer (SportMonks API)

**Base URL:** `https://api.sportmonks.com/v3/football`

### Player Profile

**Endpoint:** `GET /players/{id}`

**Example:** `https://api.sportmonks.com/v3/football/players/184798`

**Include Parameters:** `include=nationality;detailedPosition;position;metadata`

**Response Fields:**
- `id` - SportMonks player ID
- `name` - Full name
- `display_name` - Display name
- `position` - Position info
- `detailedposition` - Detailed position
- `nationality` - Nationality info
- `height` - Height in cm
- `weight` - Weight in kg
- `date_of_birth` - DOB (YYYY-MM-DD)
- `image_path` - Photo URL

### Team Profile

**Endpoint:** `GET /teams/{id}`

**Include Parameters:** `include=venue;country`

**Fields:** id, name, short_code, image_path (logo), venue, country

### Team Squad (with Jersey Numbers)

**Endpoint:** `GET /squads/seasons/{season_id}/teams/{team_id}`

Returns list of players with jersey_number for the season.

### All Teams in Season

**Endpoint:** `GET /teams/seasons/{season_id}`

---

## Images (api-sports, NBA + NFL)

**Purpose:** team logos and player headshots only. api-sports is **not**
used for box scores, stats, or fixtures — BDL and SportMonks remain the
authoritative providers for those.

### NBA — `https://v2.nba.api-sports.io`

- `GET /teams` — returns current franchises with a `logo` URL
  - Filter to `nbaFranchise=true` and `allStar=false`
- `GET /players?team={as_team_id}&season={year}` — roster per team

### NFL — `https://v1.american-football.api-sports.io`

- `GET /teams?league=1&season={year}` — teams with `logo`
- `GET /players?team={as_team_id}&season={year}` — roster per team

### Image CDN

Logos and headshot URLs are served from `media.api-sports.io` (or
upstream provider CDNs embedded in the response). Fetching those
images does **not** count toward the API quota — only the JSON
endpoint calls do.

### Quota math

- NBA: 1 (`/teams`) + ~30 (`/players` per team) = **~31 calls/run**
- NFL: 1 (`/teams`) + 32 (`/players` per team) = **33 calls/run**

Free tier is 100 requests/day (reset 00:00 UTC). Run once per season —
subsequent runs re-use the `provider_entity_map` table to skip match
work.

### Entity matching

api-sports IDs don't align with BDL IDs. First run matches by:

1. Existing `provider_entity_map` row (idempotent re-runs)
2. `teams.short_code` ↔ api-sports `code`
3. Normalized team name fallback
4. Player: `(first_name, last_name)` + team membership, DOB tiebreaker

Unmatched entities are logged, not hard-failed.

### Writes

- `teams.logo_url` and `players.photo_url` are only set when NULL
- `provider_entity_map` gets an `api-sports` row per matched entity
- Re-runs are idempotent; nothing overwrites existing values

---

## Authentication

### BallDontlie (NBA/NFL)
- **Header:** `Authorization: {api_key}`
- **Key Location:** `.env.local` -> `BALLDONTLIE_API_KEY`
- **Tier:** GOAT tier recommended (600 req/min)

### SportMonks (Football)
- **Query Parameter:** `api_token={token}`
- **Key Location:** `.env.local` -> `SPORTMONKS_API_TOKEN`
- **Rate Limit:** 3,000 requests/day

### api-sports (NBA/NFL images only)
- **Header:** `x-apisports-key: {key}`
- **Key Location:** `.env.local` -> `API_SPORTS_KEY`
- **Rate Limit:** 100 requests/day (free tier)

---

## Usage

### CLI-Driven Metadata Seeding
Run metadata seeding explicitly from the CLI when needed (for example at season start and periodic refresh points):

```bash
scoracle-seed meta seed nba --season 2025
scoracle-seed meta seed nfl --season 2025
scoracle-seed meta seed football --season 2026 --league 8
```

Optional scoped run:

```bash
scoracle-seed meta seed nfl --season 2025 --max-teams 2 --max-players 500
```

### Image Seeding (api-sports, once per season)

```bash
scoracle-seed meta images nba --season 2025
scoracle-seed meta images nfl --season 2025
```

Add `--dry-run` to verify matching and see logged field names before
writing. Note: `--dry-run` still consumes API quota (it fetches, just
doesn't write).

---

## Metadata vs Event Data

| Aspect | Metadata | Event Data |
|--------|----------|------------|
| Data Type | Photos, bio, positions | Box scores, fixtures |
| Frequency | Season start + changes | Continuous (daily) |
| Source | Profile endpoints | Game/box score endpoints |
| Priority | Important for UX | Critical for stats |

---

*Last updated: 2026-04-06*

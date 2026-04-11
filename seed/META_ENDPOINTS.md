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

## Authentication

### BallDontlie (NBA/NFL)
- **Header:** `Authorization: {api_key}`
- **Key Location:** `.env.local` -> `BALLDONTLIE_API_KEY`
- **Tier:** GOAT tier recommended (600 req/min)

### SportMonks (Football)
- **Query Parameter:** `api_token={token}`
- **Key Location:** `.env.local` -> `SPORTMONKS_API_TOKEN`
- **Rate Limit:** 3,000 requests/day

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

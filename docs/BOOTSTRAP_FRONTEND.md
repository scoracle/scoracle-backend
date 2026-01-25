# Frontend Bootstrap Data Structure

This document describes the new bootstrap data format for frontend autocomplete functionality.

## Overview

The backend exports a static JSON file (`entities_minimal.json`) containing all player and team profiles. This file should be bundled with the frontend application and loaded on startup for instant autocomplete without API calls.

## File Location

After running the export script, the file is generated at:
```
exports/entities_minimal.json
```

## Data Structure

### Root Object

```json
{
  "version": "2.0",
  "generated_at": "2026-01-16T12:00:00.000Z",
  "counts": {
    "total": 5432,
    "nba_players": 450,
    "nba_teams": 30,
    "nfl_players": 1800,
    "nfl_teams": 32,
    "football_players": 3000,
    "football_teams": 120
  },
  "entities": [...]
}
```

### Entity Object

Each entity in the `entities` array has the following structure:

```typescript
interface Entity {
  // Unique compound ID for deduplication
  id: string;  // e.g., "nba_player_237", "nfl_team_29"

  // Database ID (use this when calling API endpoints)
  entity_id: number;

  // Entity type
  type: "player" | "team";

  // Sport identifier
  sport: "NBA" | "NFL" | "FOOTBALL";

  // Display name
  name: string;

  // Lowercase, accent-normalized name for search
  normalized: string;

  // Search tokens (words from name, split for fuzzy matching)
  tokens: string[];

  // Football only: league ID for filtering
  league_id?: number;

  // Additional metadata for display
  meta: {
    // Players
    position?: string;
    position_group?: string;  // NFL only
    team?: string;            // Team abbreviation or name
    league?: string;          // Football only: league name

    // Teams
    abbreviation?: string;
    conference?: string;
    division?: string;
    country?: string;         // Football only
  }
}
```

### Example Entities

#### NBA Player
```json
{
  "id": "nba_player_237",
  "entity_id": 237,
  "type": "player",
  "sport": "NBA",
  "name": "LeBron James",
  "normalized": "lebron james",
  "tokens": ["lebron", "james"],
  "meta": {
    "position": "SF",
    "team": "LAL"
  }
}
```

#### NFL Player
```json
{
  "id": "nfl_player_2076",
  "entity_id": 2076,
  "type": "player",
  "sport": "NFL",
  "name": "Dak Prescott",
  "normalized": "dak prescott",
  "tokens": ["dak", "prescott"],
  "meta": {
    "position": "QB",
    "position_group": "Offense",
    "team": "DAL"
  }
}
```

#### Football Player (with league)
```json
{
  "id": "football_player_276",
  "entity_id": 276,
  "type": "player",
  "sport": "FOOTBALL",
  "name": "Erling Haaland",
  "normalized": "erling haaland",
  "tokens": ["erling", "haaland"],
  "league_id": 39,
  "meta": {
    "position": "Attacker",
    "team": "Manchester City",
    "league": "Premier League"
  }
}
```

#### Team
```json
{
  "id": "nba_team_31",
  "entity_id": 31,
  "type": "team",
  "sport": "NBA",
  "name": "Los Angeles Lakers",
  "normalized": "los angeles lakers",
  "tokens": ["los", "angeles", "lakers"],
  "meta": {
    "abbreviation": "LAL",
    "conference": "Western",
    "division": "Pacific"
  }
}
```

## Frontend Integration

### Loading the Data

```javascript
// Load on app startup
const bootstrapData = await import('./entities_minimal.json');
const entities = bootstrapData.entities;
```

### Setting Up Fuse.js for Fuzzy Search

```javascript
import Fuse from 'fuse.js';

const fuse = new Fuse(entities, {
  keys: [
    { name: 'name', weight: 0.7 },
    { name: 'normalized', weight: 0.5 },
    { name: 'tokens', weight: 0.3 },
    { name: 'meta.team', weight: 0.2 },
    { name: 'meta.abbreviation', weight: 0.2 },
  ],
  threshold: 0.3,
  includeScore: true,
});

// Search function
function searchEntities(query, options = {}) {
  let results = fuse.search(query);

  // Filter by sport if specified
  if (options.sport) {
    results = results.filter(r => r.item.sport === options.sport);
  }

  // Filter by type if specified
  if (options.type) {
    results = results.filter(r => r.item.type === options.type);
  }

  // Filter by league (Football only)
  if (options.league_id) {
    results = results.filter(r => r.item.league_id === options.league_id);
  }

  return results.slice(0, options.limit || 10);
}
```

### Calling the API After Selection

Once a user selects an entity from autocomplete, fetch the full profile:

```javascript
async function fetchProfile(entity) {
  const { entity_id, type, sport } = entity;

  const response = await fetch(
    `/api/v1/widget/profile/${type}/${entity_id}?sport=${sport}`
  );

  return response.json();
}
```

## Filtering by Sport

The `sport` field allows filtering entities by sport:

```javascript
// Get only NBA players
const nbaPlayers = entities.filter(e => e.sport === 'NBA' && e.type === 'player');

// Get only NFL teams
const nflTeams = entities.filter(e => e.sport === 'NFL' && e.type === 'team');

// Get Football players in Premier League
const premierLeaguePlayers = entities.filter(
  e => e.sport === 'FOOTBALL' && e.type === 'player' && e.league_id === 39
);
```

## Football League IDs

For Football (soccer), common league IDs:

| League | ID |
|--------|-----|
| Premier League | 39 |
| La Liga | 140 |
| Bundesliga | 78 |
| Serie A | 135 |
| Ligue 1 | 61 |
| MLS | 253 |

## Generating the Export

Run the export script to generate the JSON file:

```bash
python -m scoracle_data export-profiles
```

This generates `exports/entities_minimal.json` which should be copied to your frontend assets.

## Version History

- **v2.0** (2026-01-16): Sport-specific profile tables, eliminated cross-sport ID collisions
- **v1.0**: Original unified tables with sport_id filtering

## Important Notes

1. **ID Uniqueness**: The `id` field (e.g., `nba_player_237`) is globally unique. The `entity_id` (e.g., `237`) is only unique within a sport.

2. **Cross-Sport Safety**: In v2.0, player/team IDs no longer collide across sports. Previously, player ID 2801 existed in both NBA (Cade Cunningham) and FOOTBALL (a goalkeeper). Now they are completely separate tables.

3. **API Calls**: Always include the `sport` parameter when calling API endpoints. The `entity_id` alone is not sufficient.

4. **File Size**: The export is optimized for size. Full profiles (with stats, photos, etc.) are fetched on-demand via the API.

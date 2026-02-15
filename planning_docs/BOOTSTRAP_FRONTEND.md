# Frontend Bootstrap Data Structure

This document describes the new bootstrap data format for frontend autocomplete functionality.

## Overview

The backend exports per-sport static JSON files containing all player and team profiles. These files should be bundled with the frontend application and loaded on startup for instant autocomplete without API calls.

## File Location

After running the export script, per-sport files are generated at:
```
exports/nba_entities.json
exports/nfl_entities.json
exports/football_entities.json
```

## Data Structure

### Root Object

```json
{
  "version": "2.0",
  "generated_at": "2026-02-12T23:09:50.000Z",
  "sport": "NBA",
  "count": 2088,
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
  "league_id": 8,
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
// Load per-sport bootstrap data on app startup
const nbaData = await import('./nba_entities.json');
const nflData = await import('./nfl_entities.json');
const footballData = await import('./football_entities.json');

// Combine all entities for unified search
const entities = [
  ...nbaData.entities,
  ...nflData.entities,
  ...footballData.entities,
];
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

// Get Football players in Premier League (SportMonks league ID = 8)
const premierLeaguePlayers = entities.filter(
  e => e.sport === 'FOOTBALL' && e.type === 'player' && e.league_id === 8
);
```

## Football League IDs

For Football (soccer), league IDs use SportMonks IDs:

| League | ID |
|--------|-----|
| Premier League | 8 |
| Bundesliga | 82 |
| Ligue 1 | 301 |
| Serie A | 384 |
| La Liga | 564 |

## Generating the Export

Run the export script to generate the per-sport JSON files:

```bash
# Export all sports
python -m scoracle_data.cli export-profiles

# Export a single sport
python -m scoracle_data.cli export-profiles --sport FOOTBALL --season 2025

# Custom output directory
python -m scoracle_data.cli export-profiles --output ./frontend/public/data
```

This generates per-sport files in `exports/` which should be copied to your frontend assets.

## Version History

- **v2.0** (2026-02-12): Regenerated with correct BallDontLie (NBA/NFL) and SportMonks (Football) IDs. Per-sport files. Football league IDs now use SportMonks IDs (8, 82, 301, 384, 564) instead of legacy api-sports IDs.
- **v2.0** (2026-01-16): Sport-specific profile tables, eliminated cross-sport ID collisions
- **v1.0**: Original unified tables with sport_id filtering

## Important Notes

1. **ID Uniqueness**: The `id` field (e.g., `nba_player_237`) is globally unique. The `entity_id` (e.g., `237`) is only unique within a sport.

2. **Cross-Sport Safety**: Player/team IDs are scoped by sport via composite primary key `(id, sport)`. The same integer ID may exist in multiple sports.

3. **API Calls**: Always include the `sport` parameter when calling API endpoints. The `entity_id` alone is not sufficient.

4. **File Size**: The export is optimized for size. Full profiles (with stats, photos, etc.) are fetched on-demand via the API.

5. **Data Sources**: NBA and NFL entity IDs come from BallDontLie. Football entity IDs and league IDs come from SportMonks. These are NOT api-sports IDs.

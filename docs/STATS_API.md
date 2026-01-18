# Widget API Documentation

This document describes the widget endpoints that serve entity profiles and statistics.

## Overview

The widget API provides two main endpoints:

| Endpoint | Purpose |
|----------|---------|
| `/api/v1/widget/profile` | Entity info from profile tables (name, photo, team, etc.) |
| `/api/v1/widget/stats` | Stats + percentiles from stats tables |

### Key Features

- **Separate endpoints** for profile and stats (clean data separation)
- **Percentiles stored as JSONB** directly in stats tables
- **Zero/null values filtered** - only non-zero stats are returned
- **Per-36 (NBA) and Per-90 (Football)** stats included with percentile rankings
- **Small sample warning** - flag indicates when percentile comparison group is small

---

## Profile Endpoint

### `GET /api/v1/widget/profile/{entity_type}/{entity_id}`

Returns entity profile information (name, photo, team affiliation, etc.).

### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `entity_type` | string | `player` or `team` |
| `entity_id` | integer | The player or team ID |

### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `sport` | string | **Yes** | `NBA`, `NFL`, or `FOOTBALL` |

### Response Example (NBA Player)

```json
{
  "id": 237,
  "sport_id": "NBA",
  "first_name": "LeBron",
  "last_name": "James",
  "full_name": "LeBron James",
  "position": "F",
  "position_group": "Forward",
  "nationality": "USA",
  "birth_date": "1984-12-30",
  "height_inches": 81,
  "weight_lbs": 250,
  "photo_url": "https://...",
  "current_team_id": 17,
  "jersey_number": 23,
  "college": "None",
  "experience_years": 21,
  "is_active": true,
  "team": {
    "id": 17,
    "name": "Los Angeles Lakers",
    "abbreviation": "LAL",
    "logo_url": "https://...",
    "conference": "West",
    "division": "Pacific",
    "city": "Los Angeles"
  },
  "league": null
}
```

### Response Example (Football Team)

```json
{
  "id": 33,
  "sport_id": "FOOTBALL",
  "league_id": 39,
  "name": "Manchester United",
  "abbreviation": "MUN",
  "logo_url": "https://...",
  "country": "England",
  "city": "Manchester",
  "founded": 1878,
  "is_national": false,
  "venue_name": "Old Trafford",
  "venue_capacity": 74310,
  "is_active": true,
  "league": {
    "id": 39,
    "name": "Premier League",
    "country": "England",
    "logo_url": "https://..."
  }
}
```

---

## Stats Endpoint

### `GET /api/v1/widget/stats/{entity_type}/{entity_id}`

Returns statistics and percentile rankings for a player or team.

### Path Parameters

| Parameter | Type | Description |
|-----------|------|-------------|
| `entity_type` | string | `player` or `team` |
| `entity_id` | integer | The player or team ID |

### Query Parameters

| Parameter | Type | Required | Description |
|-----------|------|----------|-------------|
| `sport` | string | **Yes** | `NBA`, `NFL`, or `FOOTBALL` |
| `season` | integer | No | Season year (defaults to current season) |
| `league_id` | integer | No | League ID (for FOOTBALL only) |

---

## Response Format

```json
{
  "entity_id": 237,
  "entity_type": "player",
  "sport": "NBA",
  "season": 2025,
  "stats": {
    "games_played": 45,
    "minutes_total": 1620,
    "points_per_36": 24.5,
    "assists_per_36": 8.3,
    "rebounds_per_36": 5.2,
    "fg_pct": 0.485,
    "tp_pct": 0.382,
    "ft_pct": 0.891
  },
  "percentiles": {
    "points_per_36": 85.2,
    "assists_per_36": 72.1,
    "rebounds_per_36": 65.8,
    "fg_pct": 91.3,
    "tp_pct": 55.4,
    "ft_pct": 78.9
  },
  "percentile_metadata": {
    "position_group": "Guard",
    "sample_size": 150,
    "small_sample_warning": false
  }
}
```

### Response Fields

| Field | Type | Description |
|-------|------|-------------|
| `entity_id` | integer | Player or team ID |
| `entity_type` | string | `player` or `team` |
| `sport` | string | Sport identifier |
| `season` | integer | Season year |
| `stats` | object | All non-zero stat values |
| `percentiles` | object | Percentile rankings (0-100) for each stat |
| `percentile_metadata` | object | Comparison group info |
| `league_id` | integer | (Optional) League ID for FOOTBALL |

### Stats Object

Contains all non-zero statistics for the entity. The exact fields depend on the sport:

#### NBA Player Stats (examples)
- `games_played`, `games_started`
- `minutes_total`, `minutes_per_game`
- `points_per_36`, `assists_per_36`, `rebounds_per_36`
- `fg_pct`, `tp_pct`, `ft_pct`
- `steals_per_36`, `blocks_per_36`, `turnovers_per_36`

#### NFL Player Stats (examples)
- `games_played`, `games_started`
- `pass_yards`, `pass_touchdowns`, `passer_rating`
- `rush_yards`, `rush_touchdowns`, `yards_per_carry`
- `receptions`, `receiving_yards`, `receiving_touchdowns`
- `tackles_total`, `sacks`, `interceptions`

#### Football (Soccer) Player Stats (examples)
- `appearances`, `minutes_played`
- `goals`, `assists`, `goals_per_90`, `assists_per_90`
- `shots_total`, `shots_on_target`, `shot_accuracy`
- `passes_total`, `pass_accuracy`, `key_passes_per_90`
- `tackles`, `interceptions`, `tackles_per_90`

### Percentiles Object

Contains percentile rankings for each stat (0-100 scale):
- **100** = Best performer in comparison group
- **50** = Median performer
- **0** = Lowest performer

**Important:** Only stats with non-zero values have percentiles.

### Percentile Metadata

| Field | Type | Description |
|-------|------|-------------|
| `position_group` | string | Position group used for comparison (e.g., "Guard", "Forward") |
| `sample_size` | integer | Number of entities in the comparison group |
| `small_sample_warning` | boolean | `true` if sample_size < 20 (percentiles may be less reliable) |

**Notes:**
- For teams, `position_group` is `null` as teams are compared across the entire league.
- When `small_sample_warning` is `true`, consider displaying a visual indicator to users that the percentile comparison group is small and rankings may be less meaningful.

---

## Example Requests

### NBA Player
```bash
GET /api/v1/widget/stats/player/237?sport=NBA&season=2025
```

### NFL Team
```bash
GET /api/v1/widget/stats/team/1?sport=NFL&season=2024
```

### Football (Soccer) Player with League
```bash
GET /api/v1/widget/stats/player/1100?sport=FOOTBALL&season=2025&league_id=39
```

---

## Usage for Pizza Charts

The percentiles object is designed for easy pizza chart rendering:

```javascript
// Fetch stats
const response = await fetch('/api/v1/widget/stats/player/237?sport=NBA');
const data = await response.json();

// Extract percentiles for pizza chart
const chartData = Object.entries(data.percentiles).map(([stat, percentile]) => ({
  stat: stat,
  label: formatStatLabel(stat),  // Your formatting function
  percentile: percentile,
  value: data.stats[stat],        // Raw stat value for tooltip
}));

// Filter to key stats for display
const keyStats = ['points_per_36', 'assists_per_36', 'rebounds_per_36', 'fg_pct'];
const filteredData = chartData.filter(d => keyStats.includes(d.stat));
```

### Formatting Stat Labels

The API returns stat names as database column names (e.g., `points_per_36`). Frontend should format these for display:

```javascript
function formatStatLabel(stat) {
  const labels = {
    'points_per_36': 'Points/36',
    'assists_per_36': 'Assists/36',
    'rebounds_per_36': 'Rebounds/36',
    'fg_pct': 'FG%',
    'tp_pct': '3P%',
    'ft_pct': 'FT%',
    'goals_per_90': 'Goals/90',
    'assists_per_90': 'Assists/90',
    // ... add more as needed
  };
  return labels[stat] || stat.replace(/_/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
}
```

---

## Caching & Performance

- **ETag Support**: Use `If-None-Match` header for conditional requests (304 Not Modified)
- **Cache-Control**: Response includes `Cache-Control` header with TTL
- **Current Season**: 1-hour cache TTL
- **Historical Seasons**: 24-hour cache TTL

### Example with ETag
```javascript
// First request
const response = await fetch('/api/v1/widget/stats/player/237?sport=NBA');
const etag = response.headers.get('ETag');
const data = await response.json();

// Subsequent request (conditional)
const conditionalResponse = await fetch('/api/v1/widget/stats/player/237?sport=NBA', {
  headers: { 'If-None-Match': etag }
});

if (conditionalResponse.status === 304) {
  // Use cached data
} else {
  // New data available
  const newData = await conditionalResponse.json();
}
```

---

## Data Refresh Flow

```
1. Game completes
2. Seeder triggers based on game schedule
3. Raw stats updated in database
4. Pure Python calculates percentiles for ALL numeric stats
5. Percentiles written as JSONB to stats table
6. API serves fresh data on next request
```

Percentiles are recalculated whenever stats are seeded, ensuring they're always current.

---

## Comparison Groups

Percentiles are calculated within meaningful comparison groups:

| Sport | Player Comparison | Team Comparison |
|-------|-------------------|-----------------|
| NBA | By position group (Guard, Forward, Center) | All NBA teams |
| NFL | By position group (QB, RB, WR, etc.) | All NFL teams |
| FOOTBALL | By position group across Top 5 Leagues | All Top 5 League teams |

---

## Migration Notes

### Breaking Changes from Previous Versions

1. **`/info` endpoint removed** - Use `/profile` instead
2. **`/percentiles` endpoint removed** - Use `/stats` instead (percentiles included in response)
3. **Old unified `/profile` removed** - The old `/profile` returned info + stats + percentiles combined; now `/profile` returns entity info only
4. **New `small_sample_warning` field** - Added to `percentile_metadata`

### Current API Pattern
```javascript
// Get entity profile (name, photo, team, etc.)
const profileResponse = await fetch('/api/v1/widget/profile/player/237?sport=NBA');
const profile = await profileResponse.json();

// Get entity stats + percentiles
const statsResponse = await fetch('/api/v1/widget/stats/player/237?sport=NBA');
const { stats, percentiles, percentile_metadata } = await statsResponse.json();

// Check for small sample warning
if (percentile_metadata?.small_sample_warning) {
  console.warn('Percentiles based on small comparison group');
}
```

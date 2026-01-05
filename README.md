# Scoracle Data

Dedicated data seeding and statistics database management for Scoracle.

## Development

### Database Seeding

This repo provides comprehensive database seeding for testing and development:

#### Quick Start - Full Database Seeding

The easiest way to seed a complete test database:

```bash
python scripts/seed_database.py
```

This master script will:
1. Seed teams and players from the fixture (9 teams, 21 players across NBA, NFL, Football)
2. Seed realistic sample statistics for all players
3. Set up a complete test database ready for development

#### Individual Seeders

You can also run seeders individually:

**Entities (Teams & Players)**
```bash
python -m scoracle_data.seeders.small_dataset_seeder
```
- Fixture: [tests/fixtures/small_dataset.json](tests/fixtures/small_dataset.json)
- Seeder: [src/scoracle_data/seeders/small_dataset_seeder.py](src/scoracle_data/seeders/small_dataset_seeder.py)
- Seeds 9 teams and 21 players with realistic profile data (positions, heights, weights, etc.)

**Statistics**
```bash
python -m scoracle_data.seeders.stats_seeder
```
- Seeder: [src/scoracle_data/seeders/stats_seeder.py](src/scoracle_data/seeders/stats_seeder.py)
- Seeds realistic season statistics for all NBA players in the fixture
- Includes comprehensive stats: points, rebounds, assists, shooting percentages, advanced metrics, etc.

#### Required Environment Variables

- `DATABASE_URL` (or `NEON_DATABASE_URL`) for your Neon/Postgres database
- `API_SPORTS_KEY` for API-Sports (not used by fixture seeders, but used by the API client)

#### What Gets Seeded

**NBA:**
- 3 teams (Detroit Pistons, LA Lakers, Boston Celtics)
- 7 players with full profiles and season statistics
- Players: Cade Cunningham, Jalen Duren, Jaden Ivey, LeBron James, Anthony Davis, Jayson Tatum, Jaylen Brown

**NFL:**
- 3 teams (Dallas Cowboys, Kansas City Chiefs, San Francisco 49ers)
- 8 players with full profiles
- Players: Dak Prescott, CeeDee Lamb, Micah Parsons, Patrick Mahomes, Travis Kelce, Brock Purdy, Christian McCaffrey

**Football (Soccer):**
- 3 teams (Chelsea, Manchester United, Manchester City)
- 6 players with full profiles
- Players: Cole Palmer, Nicolas Jackson, Bruno Fernandes, Marcus Rashford, Erling Haaland, Kevin De Bruyne

#### Notes

- Do not commit secrets. Put them in `.env` locally.
- The fixture uses realistic data shapes matching API-Sports endpoints
- Statistics are sample data designed to test all stat categories and percentile calculations

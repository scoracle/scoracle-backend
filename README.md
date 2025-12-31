# Scoracle Data

Dedicated data seeding and statistics database management for Scoracle.

## Development

### Small dataset seeding (testing)

This repo includes a tiny fixture for quickly validating DB population with real API-Sports endpoint shapes:

- Fixture: [tests/fixtures/small_dataset.json](tests/fixtures/small_dataset.json)
- Seeder: [src/scoracle_data/seeders/small_dataset_seeder.py](src/scoracle_data/seeders/small_dataset_seeder.py)

**Required env vars**
- `DATABASE_URL` (or `NEON_DATABASE_URL`) for your Neon/Postgres database
- `API_SPORTS_KEY` for API-Sports (not used by the small fixture seeder yet, but used by the API client)

**Run**
- `python -m scoracle_data.seeders.small_dataset_seeder`

Notes:
- Do not commit secrets. Put them in `.env` locally.

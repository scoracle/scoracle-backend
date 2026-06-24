# Scoracle Python seeder — ingestion-only CLI (calls providers, normalizes,
# upserts raw data, then calls finalize_fixture). Referenced by the
# docker-compose `seed` service (profile: seed).
#
#   docker compose build seed
#   docker compose run --rm seed event process --sport nba --max 50
#
# psycopg ships a self-contained binary wheel (psycopg[binary]), so no libpq /
# build toolchain is needed in the image.
FROM python:3.12-slim
WORKDIR /src

# Project metadata first for layer caching, then the three installed packages
# (matches [tool.setuptools.packages.find]: scoracle_seed* / services* / shared*).
COPY pyproject.toml ./
COPY scoracle_seed/ scoracle_seed/
COPY services/ services/
COPY shared/ shared/

RUN pip install --no-cache-dir . \
 && useradd --create-home --uid 1000 scoracle

USER scoracle

# The `scoracle-seed` console script (pyproject [project.scripts]) is the
# entrypoint; `docker compose run --rm seed <args>` appends the subcommand
# (e.g. `event process --sport nba --max 50`).
ENTRYPOINT ["scoracle-seed"]

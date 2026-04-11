FROM python:3.13-slim

# System deps for psycopg binary wheels (glibc required, hence slim not alpine)
RUN apt-get update && apt-get install -y --no-install-recommends \
    libpq5 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Install dependencies first (layer cache optimization)
COPY pyproject.toml ./
COPY scoracle_seed/ scoracle_seed/
COPY services/ services/
COPY shared/ shared/
RUN pip install --no-cache-dir .

# Non-root user
RUN adduser --disabled-password --uid 1000 scoracle
USER scoracle

ENTRYPOINT ["scoracle-seed"]

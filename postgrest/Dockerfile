# PostgREST — stats and data API via Postgres views/functions
FROM postgrest/postgrest:v13.0.8
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --retries=3 \
  CMD head -c 0 /dev/tcp/localhost/3000 || exit 1

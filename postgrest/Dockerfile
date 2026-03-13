# PostgREST — stats and data API via Postgres views/functions
# The official image is scratch-based, so we copy the binary into Alpine
# to get a normal user account and the shared libs Railway expects.
FROM postgrest/postgrest:v13.0.8 AS src

FROM alpine:3.21
RUN apk add --no-cache libpq \
    && adduser -D -u 1000 -h /tmp postgrest
COPY --from=src /bin/postgrest /bin/postgrest
COPY entrypoint.sh /bin/entrypoint.sh

ENV PGRST_DB_CHANNEL_ENABLED=false \
    PGRST_DB_SCHEMAS=api \
    PGRST_DB_ANON_ROLE=web_anon \
    PGRST_OPENAPI_MODE=follow-privileges \
    PGRST_SERVER_PORT=3000

USER postgrest
EXPOSE 3000

ENTRYPOINT ["/bin/entrypoint.sh"]

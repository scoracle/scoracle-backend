# PostgREST — stats and data API via Postgres views/functions
#
# The official amd64 image is scratch-based (no OS, no NSS libs).
# Railway requires a non-root user and working getpwuid(), so we
# copy the PostgREST binary into Alpine which provides both.
FROM postgrest/postgrest:v13.0.8 AS src

FROM alpine:3.21
RUN apk add --no-cache libpq \
    && adduser -D -u 1000 -h /tmp postgrest
COPY --from=src /bin/postgrest /bin/postgrest
USER postgrest
HEALTHCHECK NONE
CMD ["sh", "-c", "export PGRST_SERVER_PORT=\"${PORT:-3000}\"; exec postgrest"]

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

# Disable the LISTEN/NOTIFY channel.  Railway's Postgres proxy kills idle
# connections every ~5-10 min, which causes a noisy reconnect loop.
# Schema reloads happen on redeploy anyway.
ENV PGRST_DB_CHANNEL_ENABLED=false

USER postgrest
EXPOSE 3000

# PostgREST writes *all* log output to stderr.  Railway classifies stderr
# as [err], which makes GitHub mark every deployment as "failed" even when
# the service is healthy.  Redirect stderr → stdout so logs appear as [inf].
CMD ["sh", "-c", "exec postgrest 2>&1"]

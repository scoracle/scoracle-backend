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
EXPOSE 3000
HEALTHCHECK --interval=10s --timeout=3s --retries=3 \
  CMD wget -qO /dev/null http://localhost:3000/ || exit 1
CMD ["postgrest"]

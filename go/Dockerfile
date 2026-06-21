# Build stage — compile all four binaries from one source tree.
FROM golang:1.25-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY cmd/ cmd/
COPY internal/ internal/
COPY docs/ docs/

# Commit/build-time stamping (audit Session 2). Pass --build-arg GIT_COMMIT=...
# to map a running container back to one source commit; defaults keep an
# unstamped local build honest rather than empty.
ARG GIT_COMMIT=docker
ARG BUILD_TIME=unknown
ARG BUILDINFO=github.com/albapepper/scoracle-data/internal/buildinfo
ENV LDFLAGS="-w -s -X ${BUILDINFO}.Commit=${GIT_COMMIT} -X ${BUILDINFO}.BuildTime=${BUILD_TIME}"

# Build the API and the three cron binaries from the same commit. Previously
# only the API was built here; the cron binaries relied on separate manual
# builds and could drift onto a different commit.
RUN CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o /out/scoracle-api    ./cmd/api \
 && CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o /out/pipeline        ./cmd/pipeline \
 && CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o /out/statcommentary  ./cmd/statcommentary \
 && CGO_ENABLED=0 go build -ldflags "$LDFLAGS" -o /out/vibesynth       ./cmd/vibesynth

# Runtime stage — minimal image. ENTRYPOINT is the API; the cron binaries ride
# along so `docker compose run` / a sidecar can invoke them at the same commit.
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
RUN adduser -D -u 1000 scoracle
USER scoracle
COPY --from=build /out/ /usr/local/bin/
EXPOSE 8000
ENTRYPOINT ["/usr/local/bin/scoracle-api"]

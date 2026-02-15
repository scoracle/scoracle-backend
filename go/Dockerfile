# Multi-stage Dockerfile for Scoracle Data Go API
# ============================================================
# Build context must be the go/ directory.
# If using Railway with dockerfilePath = "go/Dockerfile",
# set buildContext = "go" as well (or use COPY from go/ paths).
#
# Stage 1: Build the Go binary
# Stage 2: Minimal runtime image (~20MB)

# --- Build stage ---
FROM golang:1.25-alpine AS builder

RUN apk add --no-cache ca-certificates tzdata

WORKDIR /src

# Cache module downloads
COPY go.mod go.sum ./
RUN go mod download

# Copy source and build
COPY . .
RUN CGO_ENABLED=0 GOOS=linux GOARCH=amd64 \
    go build -ldflags="-s -w" -o /bin/scoracle-api ./cmd/api

# --- Runtime stage ---
FROM alpine:3.20

RUN apk add --no-cache ca-certificates tzdata

# Non-root user
RUN adduser -D -u 1000 scoracle
USER scoracle

COPY --from=builder /bin/scoracle-api /usr/local/bin/scoracle-api

EXPOSE 8000

ENTRYPOINT ["scoracle-api"]

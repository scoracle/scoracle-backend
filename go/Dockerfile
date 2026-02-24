# Build stage — compile static binary
FROM golang:1.25-alpine AS build
WORKDIR /src
COPY go.mod go.sum ./
RUN go mod download
COPY cmd/ cmd/
COPY internal/ internal/
COPY docs/ docs/
RUN CGO_ENABLED=0 go build -ldflags="-w -s" -o /scoracle-api ./cmd/api

# Runtime stage — minimal image
FROM alpine:3.21
RUN apk add --no-cache ca-certificates
RUN adduser -D -u 1000 scoracle
USER scoracle
COPY --from=build /scoracle-api /scoracle-api
EXPOSE 8000
ENTRYPOINT ["/scoracle-api"]

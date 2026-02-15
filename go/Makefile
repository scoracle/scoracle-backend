# Scoracle Data — Go backend
# ============================================================

.PHONY: build test vet lint swagger clean run-api run-seed tidy

# Build both binaries
build:
	go build -o bin/scoracle-api ./cmd/api
	go build -o bin/scoracle-ingest ./cmd/ingest

# Run all tests
test:
	go test -v -race ./...

# Go vet
vet:
	go vet ./...

# Lint (requires golangci-lint: https://golangci-lint.run/usage/install/)
lint:
	golangci-lint run ./...

# Generate swagger docs (requires swag: go install github.com/swaggo/swag/cmd/swag@latest)
swagger:
	swag init -g cmd/api/main.go -o docs --parseDependency --parseInternal

# Tidy modules
tidy:
	go mod tidy

# Clean build artifacts
clean:
	rm -rf bin/ docs/docs.go docs/swagger.json docs/swagger.yaml

# Run the API server locally
run-api: build
	./bin/scoracle-api

# Seed shortcuts
seed-nba: build
	./bin/scoracle-ingest seed nba

seed-nfl: build
	./bin/scoracle-ingest seed nfl

seed-football: build
	./bin/scoracle-ingest seed football

percentiles: build
	./bin/scoracle-ingest percentiles

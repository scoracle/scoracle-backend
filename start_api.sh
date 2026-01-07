#!/bin/bash
#
# Scoracle Data API Startup Script
#
# Usage:
#   ./start_api.sh [dev|prod]
#
# Modes:
#   dev  - Development mode with auto-reload
#   prod - Production mode with multiple workers
#

set -e

# Set PYTHONPATH
export PYTHONPATH="$(cd "$(dirname "${BASH_SOURCE[0]}")/src" && pwd)"

# Check if DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
    echo "ERROR: DATABASE_URL environment variable is not set"
    echo "Please set it before running this script:"
    echo "  export DATABASE_URL='postgresql://user:pass@host:port/db?sslmode=require'"
    exit 1
fi

# Default to development mode
MODE="${1:-dev}"

echo "Starting Scoracle Data API in $MODE mode..."
echo "PYTHONPATH: $PYTHONPATH"
echo ""

if [ "$MODE" = "prod" ]; then
    # Production mode with Gunicorn
    echo "Starting production server with Gunicorn..."

    # Set production environment variables
    export DATABASE_POOL_SIZE=${DATABASE_POOL_SIZE:-20}
    export DATABASE_POOL_MIN_SIZE=${DATABASE_POOL_MIN_SIZE:-5}

    # Install gunicorn if not installed
    if ! command -v gunicorn &> /dev/null; then
        echo "Installing gunicorn..."
        pip install gunicorn
    fi

    # Run with Gunicorn
    gunicorn scoracle_data.api.main:app \
        --workers 4 \
        --worker-class uvicorn.workers.UvicornWorker \
        --bind 0.0.0.0:8000 \
        --timeout 30 \
        --access-logfile - \
        --error-logfile -
else
    # Development mode with Uvicorn
    echo "Starting development server with Uvicorn..."
    echo "API will be available at: http://localhost:8000"
    echo "API docs: http://localhost:8000/docs"
    echo ""

    uvicorn scoracle_data.api.main:app \
        --host 0.0.0.0 \
        --port 8000 \
        --reload \
        --log-level info
fi

#!/bin/bash
set -e

echo "Preparing SQLx offline mode..."

# Check if DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
    echo "DATABASE_URL not set. Using default..."
    export DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker"
fi

echo "Using DATABASE_URL: $DATABASE_URL"

# Check if database exists, if not create it
echo "Checking database connection..."
if ! psql "$DATABASE_URL" -c '\q' 2>/dev/null; then
    echo "Database not accessible. Make sure PostgreSQL is running and database exists."
    echo "Run: createdb rain_tracker"
    exit 1
fi

# Run migrations
echo "Running migrations..."
sqlx migrate run

# Prepare offline mode
echo "Generating SQLx query metadata..."
cargo sqlx prepare

echo "✓ SQLx offline mode prepared successfully!"
echo "You can now build with: cargo build"

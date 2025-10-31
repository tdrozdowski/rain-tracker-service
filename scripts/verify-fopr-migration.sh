#!/bin/bash
# Verify FOPR tracking columns migration
# This script:
# 1. Checks database connectivity
# 2. Runs migrations (including the new FOPR tracking columns)
# 3. Verifies the new columns exist
# 4. Updates SQLx metadata cache

set -e  # Exit on error

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${YELLOW}=== FOPR Migration Verification ===${NC}\n"

# Check DATABASE_URL is set
if [ -z "$DATABASE_URL" ]; then
    echo -e "${RED}ERROR: DATABASE_URL environment variable not set${NC}"
    echo "Set it with:"
    echo "  export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker"
    exit 1
fi

echo -e "${GREEN}✓${NC} DATABASE_URL is set"

# Test database connectivity
echo -n "Testing database connection... "
if docker exec -i $(docker ps -q -f ancestor=postgres) psql "$DATABASE_URL" -c "SELECT 1;" > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Cannot connect to database${NC}"
    echo "Make sure PostgreSQL is running:"
    echo "  docker-compose up -d postgres"
    exit 1
fi

# Run migrations
echo -n "Running migrations... "
if sqlx migrate run --database-url "$DATABASE_URL" > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${RED}ERROR: Migration failed${NC}"
    exit 1
fi

# Verify FOPR tracking columns exist
echo "Verifying FOPR tracking columns:"
COLUMNS=$(docker exec -i $(docker ps -q -f ancestor=postgres) psql "$DATABASE_URL" -t -c "
    SELECT column_name, data_type, column_default
    FROM information_schema.columns
    WHERE table_name = 'gauges'
    AND column_name IN ('fopr_available', 'fopr_last_import_date', 'fopr_last_checked_date')
    ORDER BY column_name;
")

if [ -z "$COLUMNS" ]; then
    echo -e "${RED}✗ ERROR: FOPR tracking columns not found${NC}"
    exit 1
fi

echo "$COLUMNS" | while read -r line; do
    if [ ! -z "$line" ]; then
        echo -e "  ${GREEN}✓${NC} $line"
    fi
done

# Verify indexes
echo "Verifying FOPR indexes:"
INDEXES=$(docker exec -i $(docker ps -q -f ancestor=postgres) psql "$DATABASE_URL" -t -c "
    SELECT indexname
    FROM pg_indexes
    WHERE tablename = 'gauges'
    AND indexname LIKE '%fopr%'
    ORDER BY indexname;
")

if [ -z "$INDEXES" ]; then
    echo -e "${YELLOW}⚠${NC}  Warning: No FOPR indexes found"
else
    echo "$INDEXES" | while read -r idx; do
        if [ ! -z "$idx" ]; then
            echo -e "  ${GREEN}✓${NC} $idx"
        fi
    done
fi

# Update SQLx metadata cache
echo -n "Updating SQLx metadata cache... "
if cargo sqlx prepare --workspace > /dev/null 2>&1; then
    echo -e "${GREEN}✓${NC}"
else
    echo -e "${RED}✗${NC}"
    echo -e "${YELLOW}Warning: SQLx prepare failed - you may need to run this manually${NC}"
fi

echo -e "\n${GREEN}=== Migration Verification Complete ===${NC}"
echo "Next steps:"
echo "  1. Review the migration file: migrations/20250108000000_add_fopr_tracking_columns.sql"
echo "  2. Commit the migration and updated .sqlx/ directory"
echo "  3. Build the project: cargo build"

# Rain Tracker Service

A Rust-based service that tracks rain gauge readings from the Maricopa County Flood Control District and provides a RESTful API for querying rainfall data by rain year or calendar year.

## Features

- **Automated Data Collection**: Periodically fetches rain gauge data from MCFCD website
- **Postgres Storage**: Stores readings with automatic deduplication
- **REST API**: Query rainfall by rain year (Oct 1 - Sep 30) or calendar year
- **Kubernetes Ready**: Includes deployment manifests for K8s clusters
- **Production Ready**: Built with Axum, SQLx, and Tokio for performance and reliability

## API Endpoints

All endpoints are prefixed with `/api/v1`.

### Health Check
```
GET /api/v1/health
```
Returns service health status and latest reading.

### Get Water Year Readings
```
GET /api/v1/readings/{gauge_id}/water-year/{year}
```
Returns all readings for a specific gauge for a water year (Oct 1 of year-1 through Sep 30 of year).

Example: `GET /api/v1/readings/59700/water-year/2025` returns readings for gauge 59700 from Oct 1, 2024 to Sep 30, 2025.

### Get Calendar Year Readings
```
GET /api/v1/readings/{gauge_id}/calendar-year/{year}
```
Returns all readings for a specific gauge for a calendar year (Jan 1 through Dec 31).

Example: `GET /api/v1/readings/59700/calendar-year/2025` returns readings for gauge 59700 from Jan 1, 2025 to Dec 31, 2025.

### Get Latest Reading
```
GET /api/v1/readings/{gauge_id}/latest
```
Returns the most recent reading for a specific gauge.

Example: `GET /api/v1/readings/59700/latest` returns the latest reading for gauge 59700.

### Get All Gauges
```
GET /api/v1/gauges?page=1&page_size=50
```
Returns a paginated list of all rain gauges with their latest rainfall data.

Query parameters:
- `page` (optional): Page number (default: 1)
- `page_size` (optional): Number of items per page (default: 50, max: 100)

Example: `GET /api/v1/gauges?page=1&page_size=25`

### Get Gauge by ID
```
GET /api/v1/gauges/{station_id}
```
Returns detailed information for a specific gauge by its station ID.

Example: `GET /api/v1/gauges/59700` returns data for gauge 59700.

## Configuration

The service uses environment variables for configuration. Copy the example file and customize:

```bash
cp .env.example .env
```

**Important**: Edit `DATABASE_URL` in `.env` based on your setup:
- For docker-compose: use `postgres` as host (default in example)
- For local development: change to `localhost`

## Quick Start with Docker Compose

**Important**: Before running with Docker, you need to generate SQLx metadata once:

```bash
# 1. Copy environment file
cp .env.example .env
# Edit .env if needed to customize settings

# 2. First time setup - requires local PostgreSQL for SQLx preparation
createdb rain_tracker
./prepare-sqlx.sh

# 3. Now you can use docker-compose
make docker-up
make docker-logs

# Or directly with docker-compose
docker-compose up -d
docker-compose logs -f app

# Stop
make docker-down
# or: docker-compose down
```

The service will be available at `http://localhost:8080`

## Running Locally

### Prerequisites
- Rust 1.75+
- PostgreSQL 14+ (PostgreSQL 18 recommended)

### Setup Database
```bash
createdb rain_tracker
```

### Quick Setup Script
```bash
# Run the helper script to setup SQLx offline mode
./prepare-sqlx.sh
```

### Building

SQLx uses compile-time query verification. You have two options:

**Option 1: With database connection (recommended for development)**
```bash
export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker
cargo build
```

**Option 2: Offline mode (for CI/CD without database)**
```bash
# First time: Generate query metadata (requires DATABASE_URL)
cargo sqlx prepare

# Then build without database
cargo build
```

### Run Migrations
Migrations are automatically run on startup, or manually with:
```bash
sqlx migrate run
```

### Start Service
```bash
cargo run
```

## Development Workflow

### Running CI Checks Locally

To avoid CI failures, run the same checks locally before committing:

```bash
# Run all CI checks (format, clippy, tests)
make ci-check

# Or run individually:
make fmt      # Check code formatting
make clippy   # Run clippy with warnings as errors
make test     # Run all tests
```

### Pre-commit Hook

A git pre-commit hook is installed that automatically runs clippy before each commit. This prevents accidentally committing code with clippy warnings that would fail CI.

To bypass the hook (not recommended):
```bash
git commit --no-verify
```

## Running Tests

### Unit Tests
```bash
cargo test --lib
```

### Integration Tests
Requires a test database:
```bash
createdb rain_tracker_test
DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker_test cargo test
```

## Building Docker Image

```bash
docker build -t rain-tracker-service:latest .
```

## Kubernetes Deployment

### Apply Manifests
```bash
kubectl apply -f k8s/configmap.yaml
kubectl apply -f k8s/secret.yaml
kubectl apply -f k8s/deployment.yaml
kubectl apply -f k8s/service.yaml
```

### Update Secret
Edit `k8s/secret.yaml` with your actual database URL before deploying.

## Architecture

- **Fetcher Module**: Scrapes HTML table from MCFCD website using reqwest and scraper
- **Database Layer**: SQLx with Postgres for storage, supports both rain year and calendar year queries
- **Scheduler**: Tokio-based periodic task that fetches new readings every N minutes
- **API Layer**: Axum REST framework with JSON responses
- **Configuration**: Environment-based config with dotenvy

## Test Plan

### Unit Tests
- ✅ HTML parsing logic (fetcher module)
- ✅ Date range calculations (rain year logic)
- ✅ Rain reading struct parsing

### Integration Tests
- ✅ Database insert and retrieval operations
- ✅ Water year query correctness
- ✅ Calendar year query correctness
- ✅ Latest reading retrieval

### Manual Testing Checklist
- [ ] Deploy to K8s cluster
- [ ] Verify database migrations succeed
- [x] Test `/api/v1/health` endpoint returns 200
- [x] Test `/api/v1/readings/{gauge_id}/water-year/2025` returns valid data
- [x] Test `/api/v1/readings/{gauge_id}/calendar-year/2025` returns valid data
- [x] Test `/api/v1/readings/{gauge_id}/latest` returns valid data
- [x] Test `/api/v1/gauges` endpoint returns paginated gauge list
- [x] Test `/api/v1/gauges/{station_id}` returns gauge details
- [ ] Verify scheduler fetches data at configured interval
- [ ] Check logs for errors
- [ ] Verify data deduplication works (no duplicate readings)

## Development Notes

### Rain Year Calculation
A rain year starts on October 1st and ends on September 30th of the following year. For example:
- Rain Year 2025: Oct 1, 2024 - Sep 30, 2025
- Rain Year 2026: Oct 1, 2025 - Sep 30, 2026

### Data Structure
The MCFCD table contains:
- Date and Time of reading
- Cumulative rainfall (inches) for the current rain year
- Incremental rainfall (inches) for that specific reading

## Recent Changes

### v0.3.0 - Gauge-Specific Endpoints (2025)
- **Breaking Change**: All readings endpoints now require a gauge ID parameter
  - Old: `GET /api/v1/readings/water-year/{year}`
  - New: `GET /api/v1/readings/{gauge_id}/water-year/{year}`
  - Old: `GET /api/v1/readings/calendar-year/{year}`
  - New: `GET /api/v1/readings/{gauge_id}/calendar-year/{year}`
  - Old: `GET /api/v1/readings/latest`
  - New: `GET /api/v1/readings/{gauge_id}/latest`
- Added `/api/v1/gauges` endpoint for listing all gauges with pagination
- Added `/api/v1/gauges/{station_id}` endpoint for getting specific gauge details
- Simplified `/api/v1/health` endpoint to only return status (removed latest_reading field)
- Improved database queries to filter by gauge ID for better performance and consistency
- Fixed non-deterministic behavior in latest reading queries
- Updated HTTP tests to use gauge-specific endpoints

### v0.2.0 - Multi-Gauge Support (2025)
- Added support for tracking multiple rain gauges
- Implemented gauge metadata storage (name, location, elevation, etc.)
- Added 6-hour and 24-hour rainfall aggregations per gauge
- Enhanced scraper to handle multi-gauge data from MCFCD

### v0.1.0 - Initial Release
- Basic rain tracker functionality for single gauge
- Water year and calendar year queries
- Automated data collection from MCFCD website
- PostgreSQL storage with deduplication
- REST API with health check endpoint

## License

MIT

# Rain Tracker Service

A Rust-based service that tracks rain gauge readings from the Maricopa County Flood Control District and provides a RESTful API for querying rainfall data by rain year or calendar year.

## Features

- **Automated Data Collection**: Periodically fetches rain gauge data from MCFCD website
- **Postgres Storage**: Stores readings with automatic deduplication
- **REST API**: Query rainfall by rain year (Oct 1 - Sep 30) or calendar year
- **Kubernetes Ready**: Includes deployment manifests for K8s clusters
- **Production Ready**: Built with Axum, SQLx, and Tokio for performance and reliability

## API Endpoints

### Health Check
```
GET /health
```
Returns service health status and latest reading.

### Get Rain Year Readings
```
GET /readings/rain-year/{year}
```
Returns all readings for a rain year (Oct 1 of year-1 through Sep 30 of year).

Example: `GET /readings/rain-year/2025` returns readings from Oct 1, 2024 to Sep 30, 2025.

### Get Calendar Year Readings
```
GET /readings/calendar-year/{year}
```
Returns all readings for a calendar year (Jan 1 through Dec 31).

Example: `GET /readings/calendar-year/2025` returns readings from Jan 1, 2025 to Dec 31, 2025.

### Get Latest Reading
```
GET /readings/latest
```
Returns the most recent rain gauge reading.

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

### Available Make Commands
```bash
make help         # Show all available commands
make setup        # Setup database and SQLx offline mode
make build        # Build the project
make run          # Run locally
make test         # Run tests
make docker-up    # Start with docker-compose
make docker-down  # Stop docker-compose
make docker-logs  # View logs
```

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
- ✅ Rain year query correctness
- ✅ Calendar year query correctness
- ✅ Latest reading retrieval

### Manual Testing Checklist
- [ ] Deploy to K8s cluster
- [ ] Verify database migrations succeed
- [ ] Test `/health` endpoint returns 200
- [ ] Test `/readings/rain-year/2025` returns valid data
- [ ] Test `/readings/calendar-year/2025` returns valid data
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

## License

MIT

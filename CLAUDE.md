# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Overview

Rain Tracker Service is a Rust web service that periodically scrapes rain gauge data from the Maricopa County Flood Control District (MCFCD) website and exposes it via a REST API. It supports multiple gauges with water year and calendar year queries.

## Essential Commands

### Building and Running

**CRITICAL**: SQLx performs compile-time query verification. You MUST either:
1. Set `DATABASE_URL` environment variable to a live database, OR
2. Set `SQLX_OFFLINE=true` to use cached metadata from `.sqlx/` directory

Without one of these, builds will fail with database connection errors.

```bash
# Build option 1: With live database (recommended for development)
export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker
cargo build

# Build option 2: Offline mode using cached metadata (for CI/CD)
export SQLX_OFFLINE=true
cargo build

# Run locally (needs DATABASE_URL)
export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker
cargo run

# Generate SQLx metadata cache (run this after adding/modifying SQL queries)
cargo sqlx prepare --workspace
```

### Testing
```bash
# Run all tests (requires test database)
cargo test --all-targets

# Run only unit tests (lib)
cargo test --lib

# Run only integration tests
cargo test --test '*'

# Run specific test
cargo test test_name -- --nocapture
```

### CI Checks (Run before committing)
```bash
make ci-check          # Run all checks: format, clippy, tests, openapi
make fmt               # Check formatting
make clippy            # Clippy with -D warnings (exactly as CI)
make test              # All tests
make openapi           # Regenerate openapi.json
```

### Database Setup
```bash
# Create databases
createdb rain_tracker
createdb rain_tracker_test

# Run migrations (or they auto-run on service startup)
sqlx migrate run

# For Docker setup
./prepare-sqlx.sh      # One-time SQLx metadata generation
```

## Rust Code Standards

### Module Structure (Rust 2018+ Edition)

**CRITICAL**: This project uses **modern Rust module structure (Rust 2018+)**. We do **NOT** use `mod.rs` files.

#### ❌ NEVER DO THIS (Old Rust 2015 Style):
```
src/
├── mymodule/
│   ├── mod.rs          ❌ WRONG - Do not create this!
│   ├── submodule1.rs
│   └── submodule2.rs
```

#### ✅ ALWAYS DO THIS (Modern Rust 2018+ Style):
```
src/
├── mymodule.rs         ✅ Module declaration file
├── mymodule/
│   ├── submodule1.rs   ✅ Implementation files
│   └── submodule2.rs   ✅ Implementation files
```

**How it works:**
1. **Create a module file** at the parent level: `src/mymodule.rs`
2. **Declare submodules** in that file:
   ```rust
   // src/mymodule.rs
   pub mod submodule1;
   pub mod submodule2;

   // Optional: re-export commonly used items
   pub use submodule1::SomeType;
   pub use submodule2::AnotherType;
   ```
3. **Create implementation files** in the directory: `src/mymodule/*.rs`

**Real Example from this Project:**
```
src/
├── services.rs         // Declares service modules
├── services/
│   ├── reading_service.rs
│   └── gauge_service.rs
```

In `src/services.rs`:
```rust
pub mod reading_service;
pub mod gauge_service;

pub use reading_service::ReadingService;
pub use gauge_service::GaugeService;
```

**Why This Matters:**
- `mod.rs` is legacy Rust 2015 syntax (before Rust 2018 edition)
- Modern Rust uses the file-as-module pattern
- Cleaner project structure, easier navigation
- Follows current Rust best practices
- Matches what the Rust community uses today

**When Adding New Modules:**
1. Create `src/module_name.rs` for the module declaration
2. Create `src/module_name/` directory for submodules
3. Add submodule files as `src/module_name/submodule.rs`
4. Declare submodules in `src/module_name.rs` with `pub mod submodule;`
5. **NEVER create `src/module_name/mod.rs`**

**Rust Edition:**
- This project uses **Rust 2021 edition** (see `Cargo.toml`)
- Module system changed in Rust 2018
- We follow Rust 2018+ conventions

## Architecture

### High-Level Flow
```
main.rs starts service
    ↓
Spawns 2 background schedulers (Tokio tasks)
    ├─ Scheduler 1: Fetches individual gauge readings (15 min interval)
    └─ Scheduler 2: Fetches gauge list/summaries (60 min interval)
    ↓
Schedulers use Fetchers to scrape MCFCD website (reqwest + scraper)
    ↓
Data stored via Repositories (SQLx) → PostgreSQL
    ↓
API handlers query via Services (business logic layer)
    ↓
Axum serves REST API on /api/v1/*
```

### Layer Architecture (Onion/Clean)
- **API Layer** (`src/api.rs`): Axum handlers, route definitions, OpenAPI docs
- **Service Layer** (`src/services/`): Business logic, coordinates repositories
  - `ReadingService`: Water year/calendar year logic, latest reading queries
  - `GaugeService`: Gauge metadata, pagination, aggregations
- **Repository Layer** (`src/db/`): Database operations (SQLx queries)
  - `ReadingRepository`: CRUD for rain readings
  - `GaugeRepository`: CRUD for gauge metadata
- **Fetcher Layer** (`src/fetcher.rs`, `src/gauge_list_fetcher.rs`): Web scraping
- **Scheduler Layer** (`src/scheduler.rs`): Background tasks with Tokio intervals

### Key Concepts

**Water Year (Rain Year)**: Runs Oct 1 (year-1) to Sep 30 (year). Example: Water year 2025 = Oct 1, 2024 - Sep 30, 2025.

**Dual Scheduler System**: Two independent Tokio tasks run concurrently:
1. **Reading Scheduler**: Scrapes detailed readings from individual gauge pages
2. **Gauge List Scheduler**: Scrapes gauge summary/list page for metadata updates

**Deduplication**: Database has unique constraint on `(station_id, reading_date)` to prevent duplicate readings.

### Module Breakdown
- `src/config.rs`: Environment variable configuration (DATABASE_URL, GAUGE_URL, intervals, etc.)
- `src/db/models.rs`: Database models (Reading, Gauge, etc.)
- `src/fetch_error.rs`: Custom error types for HTTP/scraping failures
- `src/bin/generate-openapi.rs`: Standalone binary to generate openapi.json from code annotations

## Database

### SQLx Compile-Time Verification

**THIS IS CRITICAL AND A FREQUENT SOURCE OF BUILD FAILURES**

SQLx verifies SQL queries at compile time by connecting to a real database. Every `cargo build`, `cargo check`, `cargo clippy`, `cargo test`, etc. will fail unless you satisfy one of these requirements:

**Option 1: Live Database (Development)**
```bash
export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker
cargo build  # Now works
```

**Option 2: Offline Mode (CI/CD, Docker builds)**
```bash
export SQLX_OFFLINE=true
cargo build  # Uses cached .sqlx/ metadata instead of live DB
```

**When to regenerate .sqlx/ cache:**
- After adding new SQL queries
- After modifying existing queries
- After changing database schema

```bash
# Requires DATABASE_URL to be set
cargo sqlx prepare --workspace
git add .sqlx/
git commit -m "Update SQLx metadata"
```

**Important for CI/CD**: GitHub Actions runs `cargo sqlx prepare` to generate `.sqlx/` directory, then builds with `SQLX_OFFLINE=true` in Dockerfile.

**If you see errors like "database does not exist" or "connection refused" during build:**
1. First check if `DATABASE_URL` is set: `echo $DATABASE_URL`
2. If not set, either set it or use `SQLX_OFFLINE=true`
3. If using offline mode, ensure `.sqlx/` directory exists and is up to date

### Migrations
Located in `migrations/`. Applied automatically on service startup via `sqlx::migrate!()` in main.rs:48.

## OpenAPI Documentation

OpenAPI spec is **code-generated** from `utoipa` annotations on API handlers.

### IMPORTANT: OpenAPI Version Constraint

**We are locked to OpenAPI 3.0 (not 3.1) and utoipa 4.x (not 5.x)**

**Why?** We plan to use `progenitor` to generate Rust client code from the OpenAPI spec. Progenitor currently only supports OpenAPI 3.0, not 3.1. Unfortunately:
- utoipa 5.x only generates OpenAPI 3.1
- utoipa 4.x generates OpenAPI 3.0

**DO NOT upgrade utoipa to version 5.x** until progenitor adds OpenAPI 3.1 support.

Current versions:
- `utoipa = "4.2"` (locked to 4.x series)
- OpenAPI spec version: 3.0.x

When progenitor adds 3.1 support, we can upgrade to utoipa 5.x.

### Keeping openapi.json in Sync

**Critical**: The `openapi.json` file must stay in sync with code:
- Pre-commit hook auto-regenerates it
- CI fails if openapi.json is out of date
- Always run `make openapi` after changing API handlers/schemas

## CI/CD Pipeline

### Workflows
- `.github/workflows/ci-cd.yml`: Main pipeline (build, test, clippy, openapi, Docker push on main)
- `.github/workflows/release.yml`: Triggered on GitHub releases (builds with semver tags)

### Important CI Notes
1. **DATABASE_URL must be set** for any step that compiles code (build, clippy, generate-openapi)
2. Tests use `rain_tracker_test` database (defined in env vars)
3. Docker image uses SQLx offline mode (`.sqlx/` directory)
4. Images pushed to GitHub Container Registry (ghcr.io)

## Docker & Kubernetes

### Multi-Stage Dockerfile
- **Build stage**: `rust:1.85` - compiles release binary with SQLx offline mode
- **Runtime stage**: `debian:trixie-slim` - minimal image with ca-certificates for SSL

**SSL Certificates**: Uses Debian Trixie (13) which includes the SSL.com TLS RSA Root CA 2022 certificate needed for alert.fcd.maricopa.gov. Debian Bookworm (12) doesn't include this newer root CA and would require manual installation.

**Why Trixie instead of Bookworm?** The Maricopa County website uses an SSL.com certificate signed by a root CA that was added to Mozilla's bundle in 2023, after Debian Bookworm froze its packages. Debian Trixie (released August 2025) includes this root CA by default.

### K8s Manifests
Located in `k8s/`:
- `configmap.yaml`: Non-sensitive config (GAUGE_URL, intervals, etc.)
- `db-secrets.yaml`: Database credentials (base64 encoded)
- `deployment.yaml`: Service deployment spec
- `service.yaml`: LoadBalancer service definition

## Dependency Selection Guidelines

**IMPORTANT**: When adding new crates to this project, evaluate them against these criteria to ensure they align with our architecture and maintenance standards.

### Core Pillar Crates

Our architecture is built on these foundational crates. New dependencies MUST be compatible with:

1. **Tokio** (async runtime) - All async operations use Tokio
2. **Axum** (web framework) - HTTP server and routing
3. **SQLx** (database) - Async PostgreSQL with compile-time verification

### Evaluation Criteria for New Dependencies

Before adding a crate, verify:

#### 1. Active Maintenance ✅
- **Recent updates**: Check when the last release was published
  - ✅ Good: Updated within last 6 months
  - ⚠️ Warning: 6-12 months since last update
  - ❌ Avoid: >12 months without updates (unless very stable/mature)
- **GitHub activity**: Check recent commits, issues, PRs
  - Use: `cargo search <crate> --limit 1` then check repository
  - Look for: Active maintainer responses, bug fixes, dependency updates
- **Version**: Check if actively maintained across Rust versions
  - ✅ Good: Supports recent Rust stable (1.70+)
  - ⚠️ Warning: Requires older MSRV that conflicts with our other deps

#### 2. Architecture Compatibility 🏗️
- **Async compatibility**: Does it work with Tokio?
  - ✅ Native async support (works directly with Tokio)
  - ✅ Synchronous but can use with `tokio::task::spawn_blocking()`
  - ❌ Requires different async runtime (async-std, smol, etc.)
- **Axum integration**: If web-related, does it work with Axum?
  - Check for Axum extractors, middleware compatibility
- **SQLx compatibility**: If database-related, does it work with SQLx?
  - Must not conflict with SQLx's async model or connection pooling

#### 3. Quality Indicators 📊
- **Documentation**: Well-documented API on docs.rs
- **Examples**: Provides usage examples
- **Downloads**: Check popularity (not required, but helpful signal)
  - `cargo info <crate>` shows download stats
- **License**: Compatible with our MIT license (MIT, Apache-2.0, BSD, etc.)
- **Dependencies**: Reasonable dependency tree (avoid heavy transitive deps)
  - Use: `cargo tree -p <crate>` after adding

#### 4. Specific Use Cases

**For blocking I/O operations** (file parsing, heavy computation):
- ✅ Use with `tokio::task::spawn_blocking()`
- Example: PDF/Excel parsing crates don't need to be async

**For web scraping** (HTML parsing):
- ✅ Must work with `reqwest` (our HTTP client)
- ✅ Can be synchronous (we spawn blocking tasks)
- Current: `scraper` crate for HTML parsing

**For database operations**:
- ⚠️ MUST use SQLx - do NOT add alternative database crates
- ✅ Database-agnostic helpers (like serde serialization) are OK

**For datetime operations**:
- ✅ Must use `chrono` (already in our deps)
- ❌ Do NOT add alternative datetime crates (time, etc.)

### Adding a New Dependency - Checklist

```bash
# 1. Check crate info
cargo info <crate-name>

# 2. Verify recent updates (look for version and date)
cargo search <crate-name> --limit 1

# 3. Check repository activity (if listed in cargo info)
# Visit GitHub and check:
# - Recent commits (last 6 months)
# - Open/closed issues ratio
# - Maintainer responsiveness

# 4. Add to Cargo.toml with specific version
# Use cargo add for proper version resolution
cargo add <crate-name>

# 5. Check dependency tree for conflicts
cargo tree -p <crate-name>

# 6. Verify builds with SQLx
cargo check  # (with DATABASE_URL set or SQLX_OFFLINE=true)

# 7. Document why you added it
# Add comment in Cargo.toml explaining the use case
```

### Example: Adding Excel Parser

```toml
[dependencies]
# Excel parsing for historical data import
# Read-only, pure Rust, works with spawn_blocking()
# Last updated: 2025-09 (actively maintained)
# Repository: https://github.com/tafia/calamine
calamine = { version = "0.31", features = ["dates"] }
```

### Red Flags - Do NOT Add Crates That:

❌ Require a different async runtime (async-std, smol)
❌ Haven't been updated in >2 years (unless extremely stable)
❌ Conflict with existing core dependencies (different HTTP client, different datetime library)
❌ Have security advisories (check `cargo audit`)
❌ Are pre-1.0 with breaking changes in patch versions
❌ Duplicate functionality we already have (multiple HTTP clients, multiple JSON parsers)

### When in Doubt

If unsure about a dependency:
1. Search for alternatives: `cargo search <keyword>`
2. Check what popular projects use (Axum examples, SQLx examples)
3. Ask: "Does this integrate cleanly with Tokio/Axum/SQLx?"
4. Prefer crates from established authors (tokio-rs, serde-rs, etc.)
5. Start with minimal features, add more only if needed

## Development Workflow

### Pre-Commit Hook
Automatically runs clippy before each commit. Installed at `.git/hooks/pre-commit`. Bypass with `--no-verify` (not recommended).

### Environment Variables
Copy `.env.example` to `.env` and adjust:
- `DATABASE_URL`: Change host to `localhost` for local dev, `postgres` for Docker Compose
- `GAUGE_URL`: URL of specific gauge to scrape
- `GAUGE_LIST_URL`: URL of gauge list page
- `FETCH_INTERVAL_MINUTES`: How often to scrape readings (default: 15)
- `GAUGE_LIST_INTERVAL_MINUTES`: How often to scrape gauge list (default: 60)

### HTTP Tests
Located in `http/api-tests.http`. Uses IntelliJ HTTP Client format. CI runs these with `ijhttp` CLI tool after starting service.

## Common Gotchas

1. **BUILD FAILS: "database does not exist" / "connection refused"**: This is SQLx trying to verify queries at compile time. You MUST either:
   - Set `DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker`, OR
   - Set `SQLX_OFFLINE=true` (requires `.sqlx/` directory to exist)
   - This affects ALL cargo commands: build, check, clippy, test, run

2. **SSL certificate errors in K8s**: Ensure Dockerfile uses `debian:trixie-slim` (not bookworm) and runs `update-ca-certificates`. Debian Trixie includes the SSL.com root CA needed for alert.fcd.maricopa.gov

3. **OpenAPI CI failure**: Run `make openapi` locally and commit the generated file

   **DO NOT upgrade utoipa to 5.x** - we're locked to 4.x because progenitor doesn't support OpenAPI 3.1 yet (utoipa 5.x only generates 3.1)

4. **Tests fail with connection errors**: Ensure test database exists (`createdb rain_tracker_test`) AND set `DATABASE_URL` to point to it

5. **Docker build fails**: Run `./prepare-sqlx.sh` first to generate SQLx metadata in `.sqlx/` directory

6. **CI workflow step fails with database errors**: Every CI step that compiles code (build, clippy, generate-openapi, tests) needs `DATABASE_URL` in its `env:` section

7. **Adding new dependencies**: Before adding crates, review the **Dependency Selection Guidelines** section to ensure compatibility with our Tokio/Axum/SQLx architecture and verify the crate is actively maintained

8. **Module structure**: **NEVER create `mod.rs` files** - we use modern Rust 2018+ module structure. See the **Rust Code Standards** section for correct module organization. Use `src/module_name.rs` to declare modules, not `src/module_name/mod.rs`

## Version History

- **v0.3.0**: Gauge-specific endpoints (breaking API changes), added `/api/v1/gauges` endpoints
- **v0.2.0**: Multi-gauge support, gauge metadata, 6hr/24hr aggregations
- **v0.1.0**: Initial single-gauge implementation

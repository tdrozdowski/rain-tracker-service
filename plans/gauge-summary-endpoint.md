# Plan: Add Gauge Summary Endpoint

## Overview
Add a new endpoint to scrape gauge summary information from `https://alert.fcd.maricopa.gov/alert/Rain/ev_rain.txt` which contains information about all gauges in Maricopa County with 6-hour and 24-hour precipitation totals. This endpoint should be scraped less frequently than individual gauge readings.

## Data Source Analysis

### URL
`https://alert.fcd.maricopa.gov/alert/Rain/ev_rain.txt`

### Format
- Plain text file with fixed-width or whitespace-delimited columns
- Contains header information with report date/time
- Preliminary and unedited data

### Fields Available
1. **Gauge Name** - Station name (e.g., "4th of July Wash")
2. **City/Town** - Municipality (e.g., "Agua Caliente")
3. **Gauge ID** - Station identifier (e.g., "41200")
4. **Elevation (ft)** - Elevation in feet (e.g., "1120")
5. **Rainfall Past 6 Hours** - Decimal inches (e.g., "0.00")
6. **Rainfall Past 24 Hours** - Decimal inches (e.g., "0.00")
7. **MSP Forecast Zone** - Forecast zone or "None"
8. **General Location** - Descriptive location (e.g., "21 mi. W of Old US80...")

### Example Data
```
4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W of Old US80 on Agua Caliente Road
Columbus Wash           Agua Caliente   40800    705   0.00   0.00   None   8 mi. N of Agua Caliente
Copper Wash             Agua Caliente   41000   1080   0.00   0.00   None   15 miles north of Agua Caliente
```

## Current Architecture Analysis

### Existing Components
- **Fetcher** (`src/fetcher.rs`): Scrapes individual gauge readings from HTML pages
- **Database** (`src/db.rs`): Stores individual rain readings with `station_id` defaulting to '59700'
- **Scheduler** (`src/scheduler.rs`): Runs fetch operations every 15 minutes (configurable)
- **API** (`src/api.rs`): Exposes endpoints for water-year, calendar-year, and latest readings

### Key Observations
- Individual gauge readings are scraped frequently (15 min default)
- Database already has `station_id` field to support multiple gauges
- Current fetcher is specific to HTML parsing; new endpoint is plain text

### Architecture Issues to Address
- **Monolithic DB module**: `src/db.rs` currently contains all database operations without separation by entity
- **No repository pattern**: Direct SQL queries mixed with business logic
- **Lack of abstraction**: Adding new entities (like gauge summaries) will further clutter the single file
- **Testing difficulties**: Hard to mock or test individual entity operations independently

## Implementation Plan

### 1. Refactor Database Layer to Repository Pattern

Before adding new functionality, refactor the existing database module to follow the repository pattern. This will create a clean, maintainable structure for both existing and new entities.

#### New Directory Structure

```
src/
├── db/
│   ├── mod.rs              # Module exports and shared types
│   ├── pool.rs             # Database pool management
│   ├── error.rs            # Shared DbError type
│   ├── models.rs           # Entity models (Reading, GaugeSummary, etc.)
│   ├── reading_repository.rs    # Repository for rain_readings table
│   └── gauge_repository.rs      # Repository for gauge_summaries table (NEW)
```

#### Core Components

**`src/db/mod.rs`** - Module exports
```rust
pub mod error;
pub mod models;
pub mod pool;
pub mod reading_repository;
pub mod gauge_repository;

pub use error::DbError;
pub use models::*;
pub use pool::DbPool;
pub use reading_repository::ReadingRepository;
pub use gauge_repository::GaugeRepository;
```

**`src/db/error.rs`** - Shared error type (moved from db.rs)
```rust
#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),
}
```

**`src/db/pool.rs`** - Pool management
```rust
use sqlx::PgPool;

#[derive(Clone)]
pub struct DbPool {
    pool: PgPool,
}

impl DbPool {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }
}
```

**`src/db/models.rs`** - Entity models and API response types
```rust
use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

// Database entity models
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Reading {
    pub id: i64,
    pub reading_datetime: DateTime<Utc>,
    pub cumulative_inches: f64,
    pub incremental_inches: f64,
    pub station_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct GaugeSummary {
    pub id: i64,
    pub station_id: String,
    pub gauge_name: String,
    pub city_town: Option<String>,
    pub elevation_ft: Option<i32>,
    pub general_location: Option<String>,
    pub msp_forecast_zone: Option<String>,
    pub rainfall_past_6h_inches: Option<f64>,
    pub rainfall_past_24h_inches: Option<f64>,
    pub last_scraped_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// API response DTOs (to avoid circular dependency between services and api modules)
#[derive(Debug, Clone, Serialize)]
pub struct WaterYearSummary {
    pub water_year: i32,
    pub total_readings: usize,
    pub total_rainfall_inches: f64,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarYearSummary {
    pub calendar_year: i32,
    pub total_readings: usize,
    pub year_to_date_rainfall_inches: f64,
    pub monthly_summaries: Vec<MonthlySummary>,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlySummary {
    pub month: u32,
    pub month_name: String,
    pub readings_count: usize,
    pub monthly_rainfall_inches: f64,
    pub cumulative_ytd_inches: f64,
}
```

**`src/db/reading_repository.rs`** - Rain readings repository (refactored from existing db.rs)
```rust
use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::db::{DbError, Reading};
use crate::fetcher::RainReading;

#[derive(Clone)]
pub struct ReadingRepository {
    pool: PgPool,
}

impl ReadingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert multiple readings in a transaction
    #[instrument(skip(self, readings), fields(count = readings.len()))]
    pub async fn insert_readings(&self, readings: &[RainReading]) -> Result<usize, DbError> {
        debug!("Beginning transaction to insert {} readings", readings.len());
        let mut tx = self.pool.begin().await?;
        let mut inserted = 0;
        let mut duplicates = 0;

        for reading in readings {
            let result = sqlx::query!(
                r#"
                INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches)
                VALUES ($1, $2, $3)
                ON CONFLICT (reading_datetime, station_id) DO NOTHING
                "#,
                reading.reading_datetime,
                reading.cumulative_inches,
                reading.incremental_inches
            )
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() > 0 {
                inserted += 1;
            } else {
                duplicates += 1;
            }
        }

        tx.commit().await?;
        info!("Inserted {} new readings, {} duplicates skipped", inserted, duplicates);
        Ok(inserted)
    }

    /// Generic query to find readings within a date range
    /// Business logic for water years, calendar years, etc. should be in service layer
    #[instrument(skip(self))]
    pub async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Reading>, DbError> {
        debug!("Querying readings from {} to {}", start, end);

        let readings = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE reading_datetime >= $1 AND reading_datetime < $2
            ORDER BY reading_datetime DESC
            "#,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} readings", readings.len());
        Ok(readings)
    }

    /// Find the most recent reading
    #[instrument(skip(self))]
    pub async fn find_latest(&self) -> Result<Option<Reading>, DbError> {
        debug!("Querying for latest reading");

        let reading = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            ORDER BY reading_datetime DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await?;

        if reading.is_some() {
            debug!("Found latest reading");
        } else {
            debug!("No readings found in database");
        }

        Ok(reading)
    }
}
```

**`src/db/gauge_repository.rs`** - NEW: Gauge summaries repository
```rust
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::db::{DbError, GaugeSummary};
use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;

#[derive(Clone)]
pub struct GaugeRepository {
    pool: PgPool,
}

impl GaugeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[instrument(skip(self, summaries), fields(count = summaries.len()))]
    pub async fn upsert_summaries(&self, summaries: &[FetchedGauge]) -> Result<usize, DbError> {
        debug!("Beginning transaction to upsert {} gauge summaries", summaries.len());
        let mut tx = self.pool.begin().await?;
        let mut upserted = 0;

        for summary in summaries {
            let result = sqlx::query!(
                r#"
                INSERT INTO gauge_summaries (
                    station_id, gauge_name, city_town, elevation_ft,
                    general_location, msp_forecast_zone,
                    rainfall_past_6h_inches, rainfall_past_24h_inches,
                    last_scraped_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
                ON CONFLICT (station_id) DO UPDATE SET
                    gauge_name = EXCLUDED.gauge_name,
                    city_town = EXCLUDED.city_town,
                    elevation_ft = EXCLUDED.elevation_ft,
                    general_location = EXCLUDED.general_location,
                    msp_forecast_zone = EXCLUDED.msp_forecast_zone,
                    rainfall_past_6h_inches = EXCLUDED.rainfall_past_6h_inches,
                    rainfall_past_24h_inches = EXCLUDED.rainfall_past_24h_inches,
                    last_scraped_at = NOW(),
                    updated_at = NOW()
                "#,
                summary.station_id,
                summary.gauge_name,
                summary.city_town,
                summary.elevation_ft,
                summary.general_location,
                summary.msp_forecast_zone,
                summary.rainfall_past_6h_inches,
                summary.rainfall_past_24h_inches
            )
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() > 0 {
                upserted += 1;
            }
        }

        tx.commit().await?;
        info!("Upserted {} gauge summaries", upserted);
        Ok(upserted)
    }

    #[instrument(skip(self))]
    pub async fn count(&self) -> Result<usize, DbError> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM gauge_summaries"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.unwrap_or(0) as usize)
    }

    #[instrument(skip(self))]
    pub async fn find_paginated(
        &self,
        offset: i64,
        limit: i64
    ) -> Result<Vec<GaugeSummary>, DbError> {
        debug!("Querying gauges with offset={}, limit={}", offset, limit);

        let gauges = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            ORDER BY city_town, gauge_name
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} gauges", gauges.len());
        Ok(gauges)
    }

    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn find_by_id(&self, station_id: &str) -> Result<Option<GaugeSummary>, DbError> {
        debug!("Querying gauge by station_id");

        let gauge = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            WHERE station_id = $1
            "#,
            station_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if gauge.is_some() {
            debug!("Found gauge");
        } else {
            debug!("Gauge not found");
        }

        Ok(gauge)
    }
}
```

#### Migration Strategy

1. **Create new db/ directory structure**
2. **Move and refactor code from src/db.rs**:
   - Move `DbError` to `db/error.rs`
   - Move `Reading` to `db/models.rs`
   - Move reading operations to `db/reading_repository.rs`
   - Create pool wrapper in `db/pool.rs`
   - Keep `get_water_year()` helper function (can stay in models.rs or utils)
3. **Update imports across codebase**:
   - Change `use crate::db::RainDb` to `use crate::db::ReadingRepository`
   - Update AppState to use repositories instead of single db instance
4. **Update tests** to work with new structure

#### Benefits
- **Separation of concerns**: Each entity has its own repository
- **Testability**: Easy to mock individual repositories
- **Maintainability**: Clear where to add new operations for each entity
- **Scalability**: Adding new entities doesn't clutter existing code
- **Type safety**: Repository pattern provides clear API boundaries

**Files to create/modify**:
- NEW: `src/db/mod.rs`
- NEW: `src/db/error.rs`
- NEW: `src/db/pool.rs`
- NEW: `src/db/models.rs`
- NEW: `src/db/reading_repository.rs`
- NEW: `src/db/gauge_repository.rs`
- MODIFY: `src/api.rs` - Update to use repositories
- MODIFY: `src/scheduler.rs` - Update to use ReadingRepository
- MODIFY: `src/main.rs` - Initialize repositories
- MODIFY: `src/lib.rs` - Update module declaration
- DELETE: `src/db.rs` - Replaced by db/ directory

---

### 2. Add Service Layer for Business Logic

Create a service layer to handle business logic that doesn't belong in repositories. This keeps repositories focused on data access only.

#### New Directory Structure
```
src/
├── services/
│   ├── mod.rs               # Module exports
│   ├── reading_service.rs   # Business logic for readings
│   └── gauge_service.rs     # Business logic for gauges
```

**Note:** See sections below for full implementation details of ReadingService and GaugeService.

---

### 3. Database Schema Changes

#### Create new table: `gauge_summaries`
```sql
CREATE TABLE IF NOT EXISTS gauge_summaries (
    id BIGSERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL UNIQUE,
    gauge_name VARCHAR(255) NOT NULL,
    city_town VARCHAR(255),
    elevation_ft INTEGER,
    general_location TEXT,
    msp_forecast_zone VARCHAR(100),

    -- Recent rainfall data from the summary file
    rainfall_past_6h_inches NUMERIC(6, 2),
    rainfall_past_24h_inches NUMERIC(6, 2),

    -- Metadata
    last_scraped_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_gauge_station_id ON gauge_summaries(station_id);
CREATE INDEX idx_gauge_city_town ON gauge_summaries(city_town);
CREATE INDEX idx_gauge_last_scraped ON gauge_summaries(last_scraped_at DESC);
```

**File**: `migrations/20250103000000_create_gauge_summaries.sql`

---

### 4. Service Layer Implementation Details

This section provides the full implementation of the service layer mentioned in step 2.

**`src/services/mod.rs`** - Module exports
```rust
pub mod reading_service;
pub mod gauge_service;

pub use reading_service::ReadingService;
pub use gauge_service::GaugeService;
```

**`src/services/reading_service.rs`** - Reading business logic
```rust
use chrono::{DateTime, Datelike, NaiveDate, Utc};
use crate::db::{DbError, Reading, ReadingRepository, WaterYearSummary, CalendarYearSummary, MonthlySummary};

#[derive(Clone)]
pub struct ReadingService {
    reading_repo: ReadingRepository,
}

impl ReadingService {
    pub fn new(reading_repo: ReadingRepository) -> Self {
        Self { reading_repo }
    }

    /// Get water year summary with business logic
    pub async fn get_water_year_summary(&self, water_year: i32) -> Result<WaterYearSummary, DbError> {
        // Calculate date range (business logic)
        let (start, end) = Self::water_year_date_range(water_year);

        // Fetch data (repository)
        let readings = self.reading_repo.find_by_date_range(start, end).await?;

        // Calculate summary (business logic)
        let total_rainfall = Self::calculate_total_rainfall(&readings);

        Ok(WaterYearSummary {
            water_year,
            total_readings: readings.len(),
            total_rainfall_inches: total_rainfall,
            readings,
        })
    }

    /// Get calendar year summary with monthly breakdowns
    pub async fn get_calendar_year_summary(&self, year: i32) -> Result<CalendarYearSummary, DbError> {
        // Calculate date range (business logic)
        let (start, end) = Self::calendar_year_date_range(year);

        // Fetch data (repository)
        let mut readings = self.reading_repo.find_by_date_range(start, end).await?;

        // Sort and calculate (business logic)
        readings.sort_by_key(|r| r.reading_datetime);
        let monthly_summaries = Self::calculate_monthly_summaries(&readings);
        let year_to_date_rainfall = monthly_summaries
            .iter()
            .rev()
            .find(|m| m.readings_count > 0)
            .map(|m| m.cumulative_ytd_inches)
            .unwrap_or(0.0);

        readings.reverse(); // Back to desc for API

        Ok(CalendarYearSummary {
            calendar_year: year,
            total_readings: readings.len(),
            year_to_date_rainfall_inches: year_to_date_rainfall,
            monthly_summaries,
            readings,
        })
    }

    /// Get latest reading
    pub async fn get_latest_reading(&self) -> Result<Option<Reading>, DbError> {
        self.reading_repo.find_latest().await
    }

    // Business logic helpers (private)

    fn water_year_date_range(water_year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_date = NaiveDate::from_ymd_opt(water_year - 1, 10, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(water_year, 10, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
        let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

        (start_dt, end_dt)
    }

    fn calendar_year_date_range(year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_date = NaiveDate::from_ymd_opt(year, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(year + 1, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
        let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

        (start_dt, end_dt)
    }

    fn calculate_total_rainfall(readings: &[Reading]) -> f64 {
        readings.iter().map(|r| r.incremental_inches).sum()
    }

    fn calculate_monthly_summaries(readings: &[Reading]) -> Vec<MonthlySummary> {
        // Move the existing monthly calculation logic from api.rs get_calendar_year_summary handler
        // This includes:
        // - Group readings by month
        // - Calculate monthly totals and cumulative YTD
        // - Return Vec<MonthlySummary> with all 12 months (January-December)
        // - Handle months with no readings (0.0 values)
        // The implementation should be extracted as-is from the current api.rs
        unimplemented!("Move existing implementation from api.rs")
    }

    /// Determine which water year a date falls into
    pub fn get_water_year(date: DateTime<Utc>) -> i32 {
        let year = date.year();
        let month = date.month();

        if month >= 10 {
            year + 1
        } else {
            year
        }
    }
}
```

**`src/services/gauge_service.rs`** - Gauge business logic
```rust
use crate::db::{DbError, GaugeSummary, GaugeRepository};
use chrono::{DateTime, Utc};
use serde::Serialize;

// Pagination types (used by API)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PaginationParams {
    pub page: u32,
    pub page_size: u32,
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }

    pub fn limit(&self) -> i64 {
        self.page_size.min(100) as i64
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GaugeListResponse {
    pub total_gauges: usize,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub has_next_page: bool,
    pub has_prev_page: bool,
    pub last_scraped_at: Option<DateTime<Utc>>,
    pub gauges: Vec<GaugeSummary>,
}

#[derive(Clone)]
pub struct GaugeService {
    gauge_repo: GaugeRepository,
}

impl GaugeService {
    pub fn new(gauge_repo: GaugeRepository) -> Self {
        Self { gauge_repo }
    }

    /// Get paginated gauges with metadata
    pub async fn get_gauges_paginated(
        &self,
        params: &PaginationParams,
    ) -> Result<GaugeListResponse, DbError> {
        // Get data from repository
        let total_gauges = self.gauge_repo.count().await?;
        let gauges = self.gauge_repo
            .find_paginated(params.offset(), params.limit())
            .await?;

        // Calculate pagination metadata (business logic)
        let total_pages = ((total_gauges as f64) / (params.page_size as f64)).ceil() as u32;
        let has_next_page = params.page < total_pages;
        let has_prev_page = params.page > 1;

        let last_scraped_at = gauges.iter()
            .map(|g| g.last_scraped_at)
            .max();

        Ok(GaugeListResponse {
            total_gauges,
            page: params.page,
            page_size: params.page_size,
            total_pages,
            has_next_page,
            has_prev_page,
            last_scraped_at,
            gauges,
        })
    }

    /// Get single gauge by ID
    pub async fn get_gauge_by_id(&self, station_id: &str) -> Result<Option<GaugeSummary>, DbError> {
        self.gauge_repo.find_by_id(station_id).await
    }
}
```

**Benefits of Service Layer**:
- **Clear separation**: Repositories = data access, Services = business logic
- **Testability**: Can unit test business logic without database
- **Reusability**: Business logic can be reused across API handlers
- **Maintainability**: Easy to find and modify business rules

**Files to create**:
- NEW: `src/services/mod.rs`
- NEW: `src/services/reading_service.rs`
- NEW: `src/services/gauge_service.rs`

---

### 5. Update AppState to Use Services

After creating the service layer, update `AppState` to hold services (which internally use repositories).

**`src/api.rs`** - Update AppState
```rust
use crate::services::{ReadingService, GaugeService};

#[derive(Clone)]
pub struct AppState {
    pub reading_service: ReadingService,
    pub gauge_service: GaugeService,
}
```

Update all handlers to use services:
```rust
// Before (calling repository directly)
let readings = state.reading_repo.get_water_year_readings(year).await?;

// After (calling service)
let summary = state.reading_service.get_water_year_summary(year).await?;
```

**Files to modify**:
- `src/api.rs` - Update AppState and all handler functions
- `src/main.rs` - Initialize services and pass to AppState
- `src/lib.rs` - Add `pub mod services;`

---

### 6. Refactor Error Types (DRY Principle)

**Before implementing the new fetcher**, refactor the existing `FetchError` in `src/fetcher.rs` to be more generic and reusable.

#### Unified Error Type

Move the error enum to a shared location or make it generic enough for both fetchers:

**Option A: Shared error module** (Recommended)
```rust
// src/fetch_error.rs (NEW FILE)
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
    #[error("HTTP request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("Failed to parse data: {0}")]
    ParseError(String),
    #[error("Failed to parse date/time: {0}")]
    DateTimeError(String),
    #[error("Failed to parse number: {0}")]
    NumberError(String),
}
```

**Option B: Keep in fetcher.rs but make it more generic**
Rename `FetchError` to be clear it's for any fetching operation, and update error messages to be context-agnostic.

**Files**:
- `src/fetch_error.rs` (NEW) - Shared error type
- `src/lib.rs` - Add `pub mod fetch_error;`
- `src/fetcher.rs` - Update to `use crate::fetch_error::FetchError;`
- `src/gauge_list_fetcher.rs` - Use the same `FetchError`

---

### 7. New Fetcher Module: `gauge_list_fetcher.rs`

Create a new fetcher for parsing the plain text gauge summary file.

#### Key Components:

**Data Structures:**
```rust
use crate::fetch_error::FetchError;  // Reuse unified error type

// Note: This is the "fetcher" version of GaugeSummary (before being persisted)
// The DB model GaugeSummary (in db/models.rs) includes id, timestamps, etc.
// In the repository, import as: use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;
#[derive(Debug, Clone)]
pub struct GaugeSummary {
    pub station_id: String,
    pub gauge_name: String,
    pub city_town: Option<String>,
    pub elevation_ft: Option<i32>,
    pub rainfall_past_6h_inches: Option<f64>,
    pub rainfall_past_24h_inches: Option<f64>,
    pub msp_forecast_zone: Option<String>,
    pub general_location: Option<String>,
}

pub struct GaugeListFetcher {
    client: reqwest::Client,
    url: String,
}

impl GaugeListFetcher {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }

    pub async fn fetch_gauge_list(&self) -> Result<Vec<GaugeSummary>, FetchError> {
        // Fetch the text file
        let response = self.client.get(&self.url).send().await?;
        let text = response.text().await?;

        // Parse the content
        self.parse_text(&text)
    }

    fn parse_text(&self, text: &str) -> Result<Vec<GaugeSummary>, FetchError> {
        // Parse the plain text format
        // See "Parsing Implementation Details" section below for the actual implementation
        todo!("Implement parsing logic - see section below for strategies")
    }
}
```

**Note:** The actual parsing implementation should follow one of the strategies outlined in the "Parsing Implementation Details" section below. The parse_text method needs to be implemented based on the actual file format.

**Parsing Strategy:**
- Skip header lines until data rows begin
- Parse fixed-width or whitespace-delimited columns
- Handle missing/malformed data gracefully
- Extract all gauge records from the file

**Implementation Notes:**
- The file format appears to use multiple spaces as delimiters
- Need to identify header end and data start (likely after the column labels)
- General location field may contain multiple spaces, should be captured as remainder of line
- Handle "None" in MSP Forecast Zone field
- **Uses unified `FetchError`** - no new error type needed!
- **Note:** The source data uses "Gage" (older spelling), but we normalize to "gauge" (correct spelling) in our database and code. The parsing logic will map from the source's "gage" terminology to our "gauge" fields.

**File**: `src/gauge_list_fetcher.rs`

---

### 8. Gauge Repository (Already Created in Step 1)

The `GaugeRepository` was already created as part of the repository pattern refactoring in Step 1.

**Key Methods Available:**
- `upsert_summaries(&[FetchedGauge])` - Insert or update gauge summaries (upsert based on station_id)
- `count()` - Get total count of gauges (for pagination)
- `find_paginated(offset, limit)` - Get paginated gauge summaries ordered by city/town and name
- `find_by_id(station_id)` - Get a single gauge summary by station_id

**Models:**
- Database entity: `GaugeSummary` (in `db/models.rs`) - includes id, timestamps
- Fetcher entity: `GaugeSummary` (in `gauge_list_fetcher.rs`) - imported as `FetchedGauge`

See Step 1 above for the full implementation of `GaugeRepository`.


---

### 9. Dual-Scheduler System

Modify scheduler to support multiple fetch jobs with different intervals.

#### Approach:
- Keep existing `start_fetch_scheduler` for individual gauge readings (15 min)
- Add new `start_gauge_list_scheduler` for gauge summaries (less frequent)
- Both run concurrently using tokio tasks

#### New Scheduler Function:
```rust
use tokio::time::{self, Duration};
use tracing::{debug, error, info, instrument};

use crate::db::GaugeRepository;
use crate::gauge_list_fetcher::GaugeListFetcher;

#[instrument(skip(fetcher, gauge_repo), fields(interval_minutes = %interval_minutes))]
pub async fn start_gauge_list_scheduler(
    fetcher: GaugeListFetcher,
    gauge_repo: GaugeRepository,
    interval_minutes: u64,
) {
    let mut interval = time::interval(Duration::from_secs(interval_minutes * 60));

    info!("Gauge list scheduler started with {} minute interval", interval_minutes);

    loop {
        interval.tick().await;
        debug!("Gauge list scheduler tick - initiating fetch");

        match fetch_and_store_gauge_list(&fetcher, &gauge_repo).await {
            Ok(count) => {
                info!("Successfully fetched and stored {} gauge summaries", count);
            }
            Err(e) => {
                error!("Failed to fetch gauge list: {}", e);
            }
        }
    }
}

#[instrument(skip(fetcher, gauge_repo))]
async fn fetch_and_store_gauge_list(
    fetcher: &GaugeListFetcher,
    gauge_repo: &GaugeRepository,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching gauge list");
    let gauges = fetcher.fetch_gauge_list().await?;
    info!("Fetched {} gauges from list", gauges.len());

    debug!("Upserting gauge summaries into database");
    let upserted = gauge_repo.upsert_summaries(&gauges).await?;
    Ok(upserted)
}
```

#### New Configuration:
```rust
// In config.rs
pub struct Config {
    // ... existing fields
    pub gauge_url: String,                    // existing: individual gauge URL
    pub gauge_list_url: String,               // new: gauge list/summary URL
    pub fetch_interval_minutes: u64,          // existing: 15 min for readings
    pub gauge_list_interval_minutes: u64,     // new: e.g., 60 min for gauge list
}
```

#### Environment Variables:
```bash
# Add to .env.example
GAUGE_LIST_URL=https://alert.fcd.maricopa.gov/alert/Rain/ev_rain.txt
GAUGE_LIST_INTERVAL_MINUTES=60  # scrape gauge list hourly (less frequent)
```

**Files**:
- `src/scheduler.rs` (extend)
- `src/config.rs` (extend)
- `.env.example` (update)

---

### 10. API Endpoints

Add new routes to `src/api.rs`:

#### Endpoints:

**GET `/api/v1/gauges?page=1&page_size=50`**
- Returns paginated list of gauges with summary info
- Query parameters:
  - `page` (optional, default: 1) - Page number (1-indexed)
  - `page_size` (optional, default: 50, max: 100) - Number of results per page
- Response includes pagination metadata
- Ordered by city/town, then gauge name

**Response Example:**
```json
{
  "total_gauges": 150,
  "page": 1,
  "page_size": 50,
  "total_pages": 3,
  "has_next_page": true,
  "has_prev_page": false,
  "last_scraped_at": "2025-10-15T14:30:00Z",
  "gauges": [
    {
      "id": 1,
      "station_id": "41200",
      "gauge_name": "4th of July Wash",
      "city_town": "Agua Caliente",
      "elevation_ft": 1120,
      "general_location": "21 mi. W of Old US80 on Agua Caliente Road",
      "msp_forecast_zone": "None",
      "rainfall_past_6h_inches": 0.00,
      "rainfall_past_24h_inches": 0.00,
      "last_scraped_at": "2025-10-15T14:30:00Z",
      "created_at": "2025-10-01T00:00:00Z",
      "updated_at": "2025-10-15T14:30:00Z"
    },
    ...
  ]
}
```

**GET `/api/v1/gauges/{station_id}`**
- Returns summary info for a specific gauge
- Returns 404 if gauge not found

**Response Example:**
```json
{
  "id": 1,
  "station_id": "41200",
  "gauge_name": "4th of July Wash",
  "city_town": "Agua Caliente",
  "elevation_ft": 1120,
  "general_location": "21 mi. W of Old US80 on Agua Caliente Road",
  "msp_forecast_zone": "None",
  "rainfall_past_6h_inches": 0.00,
  "rainfall_past_24h_inches": 0.00,
  "last_scraped_at": "2025-10-15T14:30:00Z",
  "created_at": "2025-10-01T00:00:00Z",
  "updated_at": "2025-10-15T14:30:00Z"
}
```

#### New Structs:
```rust
#[derive(Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 { 1 }
fn default_page_size() -> u32 { 50 }

impl PaginationParams {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.page < 1 {
            return Err("page must be >= 1");
        }
        if self.page_size < 1 || self.page_size > 100 {
            return Err("page_size must be between 1 and 100");
        }
        Ok(())
    }

    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }

    pub fn limit(&self) -> i64 {
        self.page_size as i64
    }
}

#[derive(Serialize)]
pub struct GaugeListResponse {
    pub total_gauges: usize,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub has_next_page: bool,
    pub has_prev_page: bool,
    pub last_scraped_at: Option<DateTime<Utc>>,
    pub gauges: Vec<GaugeSummary>,
}
```

#### Handler Functions:

**Important:** Handlers should use the **service layer**, not repositories directly. This maintains proper separation of concerns and keeps business logic in services.

```rust
#[instrument(skip(state))]
async fn get_all_gauges(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<GaugeListResponse>, StatusCode> {
    // Validate pagination params
    params.validate().map_err(|e| {
        warn!("Invalid pagination params: {}", e);
        StatusCode::BAD_REQUEST
    })?;

    debug!("Fetching gauge summaries (page={}, page_size={})", params.page, params.page_size);

    // Call service layer (which handles business logic and pagination)
    let response = state.gauge_service
        .get_gauges_paginated(&params)
        .await
        .map_err(|e| {
            error!("Failed to fetch gauges: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} gauge summaries (page {}/{}, total={})",
        response.gauges.len(), response.page, response.total_pages, response.total_gauges
    );

    Ok(Json(response))
}

#[instrument(skip(state), fields(station_id = %station_id))]
async fn get_gauge_by_id(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
) -> Result<Json<GaugeSummary>, StatusCode> {
    debug!("Fetching gauge summary for station {}", station_id);

    // Call service layer
    let gauge = state.gauge_service
        .get_gauge_by_id(&station_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch gauge {}: {}", station_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Gauge {} not found", station_id);
            StatusCode::NOT_FOUND
        })?;

    info!("Retrieved gauge summary for station {}", station_id);
    Ok(Json(gauge))
}
```

#### Update Router:
```rust
pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/readings/water-year/{year}", get(get_water_year))
        .route("/readings/calendar-year/{year}", get(get_calendar_year))
        .route("/readings/latest", get(get_latest))
        .route("/gauges", get(get_all_gauges))                    // NEW
        .route("/gauges/:station_id", get(get_gauge_by_id))       // NEW
        .with_state(state);

    Router::new().nest("/api/v1", api_routes)
}
```

**File**: `src/api.rs` (extend)

---

### 11. Main Application Wiring

Restructure `src/main.rs` to support CLI commands while keeping tokio schedulers as the default.

#### Design Decision: Tokio Schedulers + CLI Commands

We'll use **tokio tasks for schedulers** (simple, single deployment) but structure the code to support **independent job execution via CLI commands**. This provides:

- ✅ Simple deployment (single binary, no extra infrastructure)
- ✅ Easy manual testing/triggering
- ✅ Clear migration path to K8s CronJobs if needed later
- ✅ Can run jobs independently without server

**When to migrate to K8s CronJobs:**
- Jobs start taking >5 minutes
- Need independent scaling/monitoring per job
- Want separate alerting per job type
- Need different schedules per environment

#### Implementation

**`src/main.rs`** - Add CLI command support:

```rust
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt::init();

    // Parse CLI arguments
    let args: Vec<String> = env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("fetch-readings") => {
            info!("Running one-time fetch of gauge readings");
            run_fetch_readings_once().await?;
        }
        Some("fetch-gauge-list") => {
            info!("Running one-time fetch of gauge list");
            run_fetch_gauge_list_once().await?;
        }
        Some("help") | Some("--help") | Some("-h") => {
            print_help();
        }
        None => {
            // Default: Run API server + both schedulers
            info!("Starting server with schedulers");
            run_server_with_schedulers().await?;
        }
        Some(cmd) => {
            eprintln!("Unknown command: {}", cmd);
            print_help();
            std::process::exit(1);
        }
    }

    Ok(())
}

fn print_help() {
    println!("Rain Tracker Service");
    println!();
    println!("USAGE:");
    println!("    rain-tracker-service [COMMAND]");
    println!();
    println!("COMMANDS:");
    println!("    (none)           Start API server with background schedulers (default)");
    println!("    fetch-readings   Fetch individual gauge readings once and exit");
    println!("    fetch-gauge-list Fetch gauge list/summary once and exit");
    println!("    help             Show this help message");
}

/// Run a single fetch of individual gauge readings
async fn run_fetch_readings_once() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let pool = connect_to_database(&config.database_url).await?;

    let fetcher = RainGaugeFetcher::new(config.gauge_url);
    let reading_repo = ReadingRepository::new(pool);

    info!("Fetching readings from {}", config.gauge_url);
    let readings = fetcher.fetch_readings().await?;
    info!("Fetched {} readings", readings.len());

    info!("Storing readings in database");
    let inserted = reading_repo.insert_readings(&readings).await?;
    info!("Successfully inserted {} readings", inserted);

    Ok(())
}

/// Run a single fetch of gauge list/summary
async fn run_fetch_gauge_list_once() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let pool = connect_to_database(&config.database_url).await?;

    let fetcher = GaugeListFetcher::new(config.gauge_list_url);
    let gauge_repo = GaugeRepository::new(pool);

    info!("Fetching gauge list from {}", config.gauge_list_url);
    let gauges = fetcher.fetch_gauge_list().await?;
    info!("Fetched {} gauges", gauges.len());

    info!("Upserting gauges in database");
    let upserted = gauge_repo.upsert_summaries(&gauges).await?;
    info!("Successfully upserted {} gauges", upserted);

    Ok(())
}

/// Run API server with both background schedulers
async fn run_server_with_schedulers() -> Result<(), Box<dyn std::error::Error>> {
    let config = Config::from_env()?;
    let pool = connect_to_database(&config.database_url).await?;

    // Initialize repositories
    let reading_repo = ReadingRepository::new(pool.clone());
    let gauge_repo = GaugeRepository::new(pool.clone());

    // Initialize services
    let reading_service = ReadingService::new(reading_repo.clone());
    let gauge_service = GaugeService::new(gauge_repo.clone());

    // Create API state
    let state = AppState {
        reading_service,
        gauge_service,
    };

    // Initialize fetchers
    let reading_fetcher = RainGaugeFetcher::new(config.gauge_url.clone());
    let gauge_list_fetcher = GaugeListFetcher::new(config.gauge_list_url.clone());

    // Spawn reading scheduler (15 min default)
    let reading_scheduler_repo = reading_repo.clone();
    tokio::spawn(async move {
        scheduler::start_fetch_scheduler(
            reading_fetcher,
            reading_scheduler_repo,
            config.fetch_interval_minutes,
        )
        .await;
    });

    // Spawn gauge list scheduler (60 min default)
    let gauge_scheduler_repo = gauge_repo.clone();
    tokio::spawn(async move {
        scheduler::start_gauge_list_scheduler(
            gauge_list_fetcher,
            gauge_scheduler_repo,
            config.gauge_list_interval_minutes,
        )
        .await;
    });

    // Start API server
    let router = create_router(state);
    let listener = tokio::net::TcpListener::bind(&config.bind_address).await?;

    info!("Server listening on {}", config.bind_address);
    axum::serve(listener, router).await?;

    Ok(())
}
```

**Testing the CLI:**

```bash
# Run server with schedulers (default)
cargo run

# Run one-time fetch of readings
cargo run -- fetch-readings

# Run one-time fetch of gauge list
cargo run -- fetch-gauge-list

# Show help
cargo run -- help
```

**Docker/K8s Usage:**

```dockerfile
# In Dockerfile, the default CMD runs the server
CMD ["./rain-tracker-service"]

# But you can override for one-time jobs
docker run rain-tracker-service fetch-readings
```

**Future K8s CronJob Migration (when needed):**

```yaml
# k8s/cronjob-fetch-readings.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: fetch-readings
spec:
  schedule: "*/15 * * * *"  # Every 15 minutes
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: fetch-readings
            image: rain-tracker-service:latest
            args: ["fetch-readings"]
          restartPolicy: OnFailure
```

**Files to modify:**
- `src/main.rs` - Complete restructure with CLI commands
- `src/lib.rs` - Add `pub mod gauge_list_fetcher;`
- `README.md` - Document CLI commands

**README.md updates:**

```markdown
## Usage

### Running the Server

By default, the service runs an API server with background schedulers:

```bash
cargo run
```

### CLI Commands

The service supports the following CLI commands for manual operations:

```bash
# Fetch individual gauge readings once and exit
cargo run -- fetch-readings

# Fetch gauge list/summary once and exit
cargo run -- fetch-gauge-list

# Show help
cargo run -- help
```

### Docker

```bash
# Run server (default)
docker run rain-tracker-service

# Run one-time fetch
docker run rain-tracker-service fetch-readings
```
```

---

### 12. Update lib.rs Module Exports

Add new module to `src/lib.rs`:
```rust
pub mod gauge_list_fetcher;
```

**File**: `src/lib.rs` (extend)

---

## Implementation Order

### Phase 1: Refactoring (Preparation)

1. **Refactor Database Layer to Repository Pattern** (src/db/ directory - NEW)
   - Create `src/db/` directory structure
   - Create `src/db/mod.rs` with module exports
   - Create `src/db/error.rs` (move `DbError` from db.rs)
   - Create `src/db/models.rs` (move `Reading` from db.rs, rename from `StoredReading`)
   - Create `src/db/pool.rs` (optional wrapper around PgPool)
   - Create `src/db/reading_repository.rs` with generic methods:
     - `insert_readings()` - batch insert
     - `find_by_date_range(start, end)` - generic date query
     - `find_latest()` - get most recent reading
   - **Remove business logic**: No date calculations, no year logic
   - **Important**: Ensure all existing functionality works after refactoring
   - Run existing tests to verify no regressions

2. **Add Service Layer** (src/services/ directory - NEW)
   - Create `src/services/` directory structure
   - Create `src/services/mod.rs` with module exports
   - Create `src/services/reading_service.rs`:
     - Move water year date calculations FROM repository
     - Move calendar year date calculations FROM repository
     - Move monthly summary calculations FROM api.rs
     - Add `get_water_year_summary()` method
     - Add `get_calendar_year_summary()` method
     - Add `get_latest_reading()` method (delegates to repo)
   - Service layer holds business logic, calls repository for data
   - Update `src/lib.rs` to add `pub mod services;`

3. **Update Application to Use Services**
   - Update `src/api.rs`: Change AppState to use `ReadingService`
   - Update all API handlers to use `state.reading_service` instead of `state.db`
   - Remove business logic from handlers (now in service layer)
   - Update `src/scheduler.rs` to use `ReadingRepository` directly (no business logic needed)
   - Update `src/main.rs`:
     - Initialize `ReadingRepository` from pool
     - Initialize `ReadingService` with repository
     - Pass service to AppState
   - Delete `src/db.rs` (now replaced by `src/db/` directory)
   - Run all tests to ensure refactoring worked correctly

4. **Refactor Error Types** (src/fetch_error.rs - NEW)
   - Create shared `FetchError` enum in new module
   - Update `src/lib.rs` to export the new module
   - Update `src/fetcher.rs` to use shared error type
   - Ensure existing tests still pass

### Phase 2: New Features (Gauge Summaries)

5. **Database Migration** (migrations/20250103000000_create_gauge_summaries.sql)
   - Create `gauge_summaries` table with all fields
   - Add indexes (station_id, city_town, last_scraped_at)
   - Run migration

6. **Gauge Summary Model** (src/db/models.rs)
   - Add `GaugeSummary` struct to existing models.rs (rename from `StoredGaugeSummary`)

7. **Gauge Repository** (src/db/gauge_repository.rs - NEW)
   - Create `GaugeRepository` struct
   - Keep it simple - only data access methods:
     - `upsert_summaries()` - insert/update with ON CONFLICT
     - `count()` - get total count
     - `find_paginated(offset, limit)` - query with LIMIT/OFFSET
     - `find_by_id(station_id)` - query single gauge
   - **No business logic**: No pagination calculations, those go in service
   - Add to `src/db/mod.rs` exports

8. **Gauge Service** (src/services/gauge_service.rs - NEW)
   - Create `GaugeService` struct
   - Add business logic methods:
     - `get_gauges_paginated(params)` - pagination calculations + repo call
     - `get_gauge_by_id(station_id)` - simple delegation to repo
   - Add to `src/services/mod.rs` exports

9. **Gauge List Fetcher** (src/gauge_list_fetcher.rs - NEW)
   - Create `GaugeSummary` struct (fetcher version, no id/timestamps)
   - Create `GaugeListFetcher` with text parsing logic
   - **Use unified `FetchError`** from step 4
   - Handle fixed-width/whitespace-delimited format
   - Write unit tests with sample data
   - Add to `src/lib.rs` module exports

10. **Configuration** (src/config.rs and .env.example)
    - Add `gauge_list_url` field to `Config`
    - Add `gauge_list_interval_minutes` field to `Config`
    - Update `.env.example` with new variables and defaults

11. **Gauge List Scheduler** (src/scheduler.rs)
    - Add `start_gauge_list_scheduler` function
    - Create `fetch_and_store_gauge_list` helper function
    - Pass `GaugeRepository` to scheduler (no service needed, just data insert)

12. **API Routes** (src/api.rs)
    - Update `AppState` to include `gauge_service: GaugeService`
    - Add `PaginationParams` struct with validation
    - Add `GaugeListResponse` struct (with pagination metadata)
    - Add `get_all_gauges` handler (calls service)
    - Add `get_gauge_by_id` handler (calls service)
    - Add routes to router: `/gauges` and `/gauges/:station_id`

13. **Main Application Wiring** (src/main.rs)
    - Restructure main.rs to support CLI commands:
      - Add `run_fetch_readings_once()` function for one-time reading fetch
      - Add `run_fetch_gauge_list_once()` function for one-time gauge list fetch
      - Add `run_server_with_schedulers()` function for default server mode
      - Add CLI argument parsing (fetch-readings, fetch-gauge-list, help)
    - In server mode:
      - Initialize `GaugeRepository` from pool
      - Initialize `GaugeService` with repository
      - Initialize `GaugeListFetcher`
      - Update `AppState` to include both services (reading and gauge)
      - Spawn both scheduler tasks (readings and gauge list) concurrently
    - Test CLI commands work independently

### Phase 3: Testing

14. **Testing** - See comprehensive test plan below

---

## Comprehensive Test Plan

### Testing Strategy Overview

This plan uses a combination of:
- **Unit tests** - Fast, isolated tests for business logic and parsing
- **Integration tests** - Database and HTTP integration tests
- **Mocking** - Using `mockall` crate for repository mocking in service tests

### Mocking Library Setup

**Add to `Cargo.toml`:**
```toml
[dev-dependencies]
mockall = "0.12"
tokio-test = "0.4"
```

**Enable mocking in repositories:**
```rust
// In src/db/reading_repository.rs and src/db/gauge_repository.rs
#[cfg_attr(test, mockall::automock)]
pub trait ReadingRepositoryTrait {
    async fn insert_readings(&self, readings: &[RainReading]) -> Result<usize, DbError>;
    async fn find_by_date_range(&self, start: DateTime<Utc>, end: DateTime<Utc>) -> Result<Vec<Reading>, DbError>;
    async fn find_latest(&self) -> Result<Option<Reading>, DbError>;
}

// Implement trait for actual repository
impl ReadingRepositoryTrait for ReadingRepository {
    // ... actual implementation
}
```

---

### 1. Unit Tests: Repository Layer

**Location**: `src/db/reading_repository.rs`, `src/db/gauge_repository.rs`

**What to test**: Basic query logic (without business logic)

**`tests/db/reading_repository_test.rs`**
```rust
use sqlx::PgPool;
use chrono::{DateTime, Utc};

#[sqlx::test]
async fn test_insert_readings(pool: PgPool) {
    let repo = ReadingRepository::new(pool.clone());

    let readings = vec![
        RainReading {
            reading_datetime: Utc::now(),
            cumulative_inches: 1.5,
            incremental_inches: 0.5,
        },
    ];

    let result = repo.insert_readings(&readings).await;
    assert!(result.is_ok());
    assert_eq!(result.unwrap(), 1);
}

#[sqlx::test]
async fn test_find_by_date_range(pool: PgPool) {
    let repo = ReadingRepository::new(pool.clone());

    // Insert test data
    // ...

    let start = Utc::now() - chrono::Duration::days(7);
    let end = Utc::now();

    let readings = repo.find_by_date_range(start, end).await.unwrap();
    assert!(readings.len() > 0);
}

#[sqlx::test]
async fn test_find_latest(pool: PgPool) {
    let repo = ReadingRepository::new(pool.clone());

    // Insert test data with known timestamps
    // ...

    let latest = repo.find_latest().await.unwrap();
    assert!(latest.is_some());
}
```

**`tests/db/gauge_repository_test.rs`**
```rust
#[sqlx::test]
async fn test_upsert_summaries_insert(pool: PgPool) {
    let repo = GaugeRepository::new(pool.clone());

    let summaries = vec![
        GaugeSummary {
            station_id: "12345".to_string(),
            gauge_name: "Test Gauge".to_string(),
            // ... other fields
        },
    ];

    let count = repo.upsert_summaries(&summaries).await.unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test]
async fn test_upsert_summaries_update(pool: PgPool) {
    let repo = GaugeRepository::new(pool.clone());

    // Insert initial data
    // ...

    // Update with different values
    let updated = vec![
        GaugeSummary {
            station_id: "12345".to_string(),
            gauge_name: "Updated Gauge".to_string(),
            // ... other fields
        },
    ];

    let count = repo.upsert_summaries(&updated).await.unwrap();

    // Verify update occurred
    let gauge = repo.find_by_id("12345").await.unwrap().unwrap();
    assert_eq!(gauge.gauge_name, "Updated Gauge");
}

#[sqlx::test]
async fn test_find_paginated(pool: PgPool) {
    let repo = GaugeRepository::new(pool.clone());

    // Insert 100 test gauges
    // ...

    let page1 = repo.find_paginated(0, 50).await.unwrap();
    assert_eq!(page1.len(), 50);

    let page2 = repo.find_paginated(50, 50).await.unwrap();
    assert_eq!(page2.len(), 50);
}

#[sqlx::test]
async fn test_count(pool: PgPool) {
    let repo = GaugeRepository::new(pool.clone());

    // Insert known number of gauges
    // ...

    let count = repo.count().await.unwrap();
    assert_eq!(count, 100);
}
```

---

### 2. Unit Tests: Service Layer (with Mocking)

**Location**: `src/services/reading_service.rs`, `src/services/gauge_service.rs`

**What to test**: Business logic without database dependency

**`tests/services/reading_service_test.rs`**
```rust
use mockall::predicate::*;
use chrono::{TimeZone, Utc};

#[tokio::test]
async fn test_get_water_year_summary() {
    // Create mock repository
    let mut mock_repo = MockReadingRepositoryTrait::new();

    // Setup expectations
    mock_repo
        .expect_find_by_date_range()
        .returning(|_, _| {
            Ok(vec![
                Reading {
                    id: 1,
                    reading_datetime: Utc.with_ymd_and_hms(2024, 10, 15, 12, 0, 0).unwrap(),
                    cumulative_inches: 5.0,
                    incremental_inches: 0.5,
                    station_id: "59700".to_string(),
                    created_at: Utc::now(),
                },
                Reading {
                    id: 2,
                    reading_datetime: Utc.with_ymd_and_hms(2024, 11, 1, 12, 0, 0).unwrap(),
                    cumulative_inches: 5.5,
                    incremental_inches: 0.5,
                    station_id: "59700".to_string(),
                    created_at: Utc::now(),
                },
            ])
        });

    let service = ReadingService::new(mock_repo);

    let summary = service.get_water_year_summary(2025).await.unwrap();

    assert_eq!(summary.water_year, 2025);
    assert_eq!(summary.total_readings, 2);
    assert_eq!(summary.total_rainfall_inches, 1.0); // 0.5 + 0.5
}

#[test]
fn test_water_year_date_range() {
    let (start, end) = ReadingService::water_year_date_range(2025);

    assert_eq!(start.year(), 2024);
    assert_eq!(start.month(), 10);
    assert_eq!(start.day(), 1);

    assert_eq!(end.year(), 2025);
    assert_eq!(end.month(), 10);
    assert_eq!(end.day(), 1);
}

#[test]
fn test_calculate_total_rainfall() {
    let readings = vec![
        Reading {
            incremental_inches: 0.5,
            // ... other fields
        },
        Reading {
            incremental_inches: 0.3,
            // ... other fields
        },
        Reading {
            incremental_inches: 0.2,
            // ... other fields
        },
    ];

    let total = ReadingService::calculate_total_rainfall(&readings);
    assert_eq!(total, 1.0);
}

#[test]
fn test_calculate_monthly_summaries() {
    // Test water year boundary logic
    let readings = vec![
        // September reading
        create_test_reading(2024, 9, 30, 10.0, 1.0),
        // October reading (new water year)
        create_test_reading(2024, 10, 15, 1.5, 1.5),
    ];

    let summaries = ReadingService::calculate_monthly_summaries(&readings);

    let sept = summaries.iter().find(|s| s.month == 9).unwrap();
    let oct = summaries.iter().find(|s| s.month == 10).unwrap();

    // October should include Sept's cumulative + its own
    assert_eq!(oct.cumulative_ytd_inches, 10.0 + 1.5);
}

#[test]
fn test_get_water_year() {
    let oct_date = Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap();
    assert_eq!(ReadingService::get_water_year(oct_date), 2025);

    let sept_date = Utc.with_ymd_and_hms(2024, 9, 30, 23, 59, 59).unwrap();
    assert_eq!(ReadingService::get_water_year(sept_date), 2024);
}
```

**`tests/services/gauge_service_test.rs`**
```rust
#[tokio::test]
async fn test_get_gauges_paginated() {
    let mut mock_repo = MockGaugeRepositoryTrait::new();

    // Mock count
    mock_repo
        .expect_count()
        .returning(|| Ok(150));

    // Mock paginated results
    mock_repo
        .expect_find_paginated()
        .returning(|offset, limit| {
            // Return mock gauges
            Ok(vec![
                GaugeSummary {
                    id: offset as i64 + 1,
                    station_id: format!("{}", offset + 1),
                    gauge_name: "Test Gauge".to_string(),
                    // ... other fields
                },
            ])
        });

    let service = GaugeService::new(mock_repo);

    let params = PaginationParams {
        page: 1,
        page_size: 50,
    };

    let response = service.get_gauges_paginated(&params).await.unwrap();

    assert_eq!(response.total_gauges, 150);
    assert_eq!(response.page, 1);
    assert_eq!(response.page_size, 50);
    assert_eq!(response.total_pages, 3);
    assert_eq!(response.has_next_page, true);
    assert_eq!(response.has_prev_page, false);
}

#[tokio::test]
async fn test_get_gauges_paginated_last_page() {
    let mut mock_repo = MockGaugeRepositoryTrait::new();

    mock_repo.expect_count().returning(|| Ok(125));
    mock_repo.expect_find_paginated().returning(|_, _| Ok(vec![]));

    let service = GaugeService::new(mock_repo);

    let params = PaginationParams {
        page: 3,
        page_size: 50,
    };

    let response = service.get_gauges_paginated(&params).await.unwrap();

    assert_eq!(response.total_pages, 3);
    assert_eq!(response.has_next_page, false);
    assert_eq!(response.has_prev_page, true);
}
```

---

### 3. Unit Tests: Fetchers (Parsing Logic)

**Location**: `src/fetcher.rs`, `src/gauge_list_fetcher.rs`

**What to test**: HTML/text parsing without network calls

**`src/fetcher.rs` (existing tests remain)**
```rust
#[test]
fn test_parse_reading() {
    let fetcher = RainGaugeFetcher::new("".to_string());
    let result = fetcher.parse_reading("10/14/2025", "06:00:00", "1.85", "0.00");
    assert!(result.is_ok());

    let reading = result.unwrap();
    assert_eq!(reading.cumulative_inches, 1.85);
    assert_eq!(reading.incremental_inches, 0.0);
}

#[test]
fn test_parse_html_with_real_sample() {
    let html = include_str!("../http/httpRequests/2025-10-14T135928.200.html");
    let fetcher = RainGaugeFetcher::new("".to_string());
    let result = fetcher.parse_html(html);

    assert!(result.is_ok());
    let readings = result.unwrap();
    assert!(readings.len() > 100);
}
```

**`src/gauge_list_fetcher.rs` (NEW tests)**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gauge_line_valid() {
        let fetcher = GaugeListFetcher::new("".to_string());
        let line = "4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W of Old US80 on Agua Caliente Road";

        let result = fetcher.parse_gauge_line(line);
        assert!(result.is_ok());

        let gauge = result.unwrap();
        assert_eq!(gauge.station_id, "41200");
        assert_eq!(gauge.gauge_name, "4th of July Wash");
        assert_eq!(gauge.city_town, Some("Agua Caliente".to_string()));
        assert_eq!(gauge.elevation_ft, Some(1120));
        assert_eq!(gauge.rainfall_past_6h_inches, Some(0.00));
        assert_eq!(gauge.rainfall_past_24h_inches, Some(0.00));
        assert_eq!(gauge.general_location, Some("21 mi. W of Old US80 on Agua Caliente Road".to_string()));
    }

    #[test]
    fn test_parse_gauge_line_missing_fields() {
        let fetcher = GaugeListFetcher::new("".to_string());
        let line = "Test Gauge   Phoenix   12345";

        let result = fetcher.parse_gauge_line(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_text_with_headers() {
        let text = r#"
Precipitation Gauge Report
Date: 10/15/25 0818

Gauge Name              City/Town       ID      Elev   6hr    24hr   Zone   Location
4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W of Old US80 on Agua Caliente Road
Columbus Wash           Agua Caliente   40800    705   0.00   0.00   None   8 mi. N of Agua Caliente
        "#;

        let fetcher = GaugeListFetcher::new("".to_string());
        let result = fetcher.parse_text(text);

        assert!(result.is_ok());
        let gauges = result.unwrap();
        assert_eq!(gauges.len(), 2);
    }

    #[test]
    fn test_parse_text_skips_empty_lines() {
        let text = r#"
4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W

Columbus Wash           Agua Caliente   40800    705   0.00   0.00   None   8 mi. N
        "#;

        let fetcher = GaugeListFetcher::new("".to_string());
        let result = fetcher.parse_text(text);

        assert!(result.is_ok());
        let gauges = result.unwrap();
        assert_eq!(gauges.len(), 2);
    }

    #[test]
    fn test_is_header_line() {
        assert!(is_header_line("Precipitation Gauge Report"));
        assert!(is_header_line("Gauge Name              City/Town"));
        assert!(is_header_line(""));
        assert!(!is_header_line("Columbus Wash           Agua Caliente   40800"));
    }
}
```

---

### 4. Unit Tests: API Layer

**Location**: `src/api.rs`

**What to test**: Request validation, response serialization

**`src/api.rs` tests**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pagination_params_defaults() {
        let json = serde_json::json!({});
        let params: PaginationParams = serde_json::from_value(json).unwrap();

        assert_eq!(params.page, 1);
        assert_eq!(params.page_size, 50);
    }

    #[test]
    fn test_pagination_params_validate_valid() {
        let params = PaginationParams {
            page: 2,
            page_size: 75,
        };

        assert!(params.validate().is_ok());
    }

    #[test]
    fn test_pagination_params_validate_page_too_low() {
        let params = PaginationParams {
            page: 0,
            page_size: 50,
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn test_pagination_params_validate_page_size_too_high() {
        let params = PaginationParams {
            page: 1,
            page_size: 101,
        };

        assert!(params.validate().is_err());
    }

    #[test]
    fn test_pagination_params_offset() {
        let params = PaginationParams {
            page: 3,
            page_size: 50,
        };

        assert_eq!(params.offset(), 100);
    }

    #[test]
    fn test_pagination_params_limit() {
        let params = PaginationParams {
            page: 1,
            page_size: 25,
        };

        assert_eq!(params.limit(), 25);
    }
}
```

---

### 5. API Testing: JetBrains HTTP Client

**Location**: `http/api-tests.http`

**What to test**: Real API endpoints against running service (manual testing + validation)

**Why HTTP Client instead of Rust integration tests:**
- ✅ Simpler: No TestServer boilerplate or test database setup
- ✅ Real integration: Tests actual running service
- ✅ Better DX: Run directly in IDE, already using for existing endpoints
- ✅ Less maintenance: Minimal code, no dependencies like `axum_test`
- ✅ Living documentation: Serves as API examples

**Trade-offs:**
- ❌ Manual execution (not in `cargo test`)
- ❌ Requires running server first
- ✅ Can automate in CI later with `intellij-http-client` CLI if needed

**Extend `http/api-tests.http` with new gauge endpoints:**

```http
###############################################################################
# Gauge Endpoints
###############################################################################

### Get All Gauges (Default pagination - page 1, 50 per page)
GET {{baseUrl}}/api/v1/gauges

> {%
    client.test("Get all gauges returns 200", function() {
        client.assert(response.status === 200, "Response status is not 200");
    });

    client.test("Get all gauges has correct structure", function() {
        client.assert(response.body.hasOwnProperty("total_gauges"), "Missing total_gauges");
        client.assert(response.body.hasOwnProperty("page"), "Missing page");
        client.assert(response.body.page === 1, "Page is not 1");
        client.assert(response.body.hasOwnProperty("page_size"), "Missing page_size");
        client.assert(response.body.page_size === 50, "Page size is not 50");
        client.assert(response.body.hasOwnProperty("total_pages"), "Missing total_pages");
        client.assert(response.body.hasOwnProperty("has_next_page"), "Missing has_next_page");
        client.assert(response.body.hasOwnProperty("has_prev_page"), "Missing has_prev_page");
        client.assert(response.body.has_prev_page === false, "First page should not have prev");
        client.assert(response.body.hasOwnProperty("gauges"), "Missing gauges");
        client.assert(Array.isArray(response.body.gauges), "Gauges is not an array");
    });

    client.test("Gauge objects have correct structure", function() {
        if (response.body.gauges.length > 0) {
            const gauge = response.body.gauges[0];
            client.assert(gauge.hasOwnProperty("id"), "Gauge missing id");
            client.assert(gauge.hasOwnProperty("station_id"), "Gauge missing station_id");
            client.assert(gauge.hasOwnProperty("gauge_name"), "Gauge missing gauge_name");
            client.assert(gauge.hasOwnProperty("city_town"), "Gauge missing city_town");
            client.assert(gauge.hasOwnProperty("elevation_ft"), "Gauge missing elevation_ft");
            client.assert(gauge.hasOwnProperty("rainfall_past_6h_inches"), "Gauge missing rainfall_past_6h_inches");
            client.assert(gauge.hasOwnProperty("rainfall_past_24h_inches"), "Gauge missing rainfall_past_24h_inches");
            client.assert(gauge.hasOwnProperty("last_scraped_at"), "Gauge missing last_scraped_at");
        }
    });
%}

###

### Get All Gauges (Custom pagination - page 2, 25 per page)
GET {{baseUrl}}/api/v1/gauges?page=2&page_size=25

> {%
    client.test("Custom pagination returns 200", function() {
        client.assert(response.status === 200, "Response status is not 200");
    });

    client.test("Custom pagination has correct values", function() {
        client.assert(response.body.page === 2, "Page is not 2");
        client.assert(response.body.page_size === 25, "Page size is not 25");
        client.assert(response.body.gauges.length <= 25, "Too many gauges returned");
    });

    client.test("Pagination metadata is consistent", function() {
        const totalPages = Math.ceil(response.body.total_gauges / 25);
        client.assert(response.body.total_pages === totalPages, "Total pages calculation incorrect");

        if (response.body.page < response.body.total_pages) {
            client.assert(response.body.has_next_page === true, "Should have next page");
        }
        if (response.body.page > 1) {
            client.assert(response.body.has_prev_page === true, "Should have prev page");
        }
    });
%}

###

### Get Gauge by ID (Valid - known gauge)
GET {{baseUrl}}/api/v1/gauges/59700

> {%
    client.test("Get gauge by ID returns 200", function() {
        client.assert(response.status === 200, "Response status is not 200");
    });

    client.test("Get gauge by ID returns correct structure", function() {
        client.assert(response.body.hasOwnProperty("station_id"), "Missing station_id");
        client.assert(response.body.station_id === "59700", "Station ID doesn't match");
        client.assert(response.body.hasOwnProperty("gauge_name"), "Missing gauge_name");
        client.assert(response.body.hasOwnProperty("city_town"), "Missing city_town");
    });
%}

###

### Get Gauge by ID (Not Found)
GET {{baseUrl}}/api/v1/gauges/99999

> {%
    client.test("Get non-existent gauge returns 404", function() {
        client.assert(response.status === 404, "Response status is not 404");
    });
%}

###

### Get Gauges with Invalid Pagination (page = 0)
GET {{baseUrl}}/api/v1/gauges?page=0&page_size=50

> {%
    client.test("Invalid page number returns 400", function() {
        client.assert(response.status === 400, "Response status is not 400");
    });
%}

###

### Get Gauges with Invalid Pagination (page_size > 100)
GET {{baseUrl}}/api/v1/gauges?page=1&page_size=101

> {%
    client.test("Page size too large returns 400", function() {
        client.assert(response.status === 400, "Response status is not 400");
    });
%}

###

### Get Gauges with Invalid Pagination (page_size = 0)
GET {{baseUrl}}/api/v1/gauges?page=1&page_size=0

> {%
    client.test("Page size zero returns 400", function() {
        client.assert(response.status === 400, "Response status is not 400");
    });
%}

###
```

**How to run:**
1. Start the server: `cargo run`
2. Open `http/api-tests.http` in RustRover
3. Click the green play button next to any request
4. View results in the HTTP Client tool window

**Optional CI Integration (future):**

You can automate HTTP Client tests in CI using the official JetBrains tooling:

**Option 1: GitHub Action (requires docker-compose)**

```yaml
# .github/workflows/api-tests.yml
name: API Tests

on:
  push:
    branches: [ main, master ]
  pull_request:

jobs:
  api-tests:
    runs-on: ubuntu-latest

    steps:
      - uses: actions/checkout@v3

      - name: Create .env file for docker-compose
        run: |
          cat > .env << EOF
          DATABASE_URL=postgres://postgres:postgres@db:5432/rain_tracker
          GAUGE_URL=https://alert.fcd.maricopa.gov/php/showdata4.php?ID=59700&NM=1000
          GAUGE_LIST_URL=https://alert.fcd.maricopa.gov/alert/Rain/ev_rain.txt
          FETCH_INTERVAL_MINUTES=15
          GAUGE_LIST_INTERVAL_MINUTES=60
          BIND_ADDRESS=0.0.0.0:3000
          EOF

      - name: Start services with docker-compose
        run: |
          docker-compose up -d
          # Wait for service to be healthy
          timeout 60 bash -c 'until curl -f http://localhost:3000/api/v1/health; do sleep 2; done'

      - name: Run migrations (if needed)
        run: |
          docker-compose exec -T app sqlx migrate run || true

      - name: Run HTTP Client tests
        uses: jetbrains/intellij-http-client-action@v1
        with:
          files: http/api-tests.http
          env-file: http/http-client.env.json
          env: ci  # Use 'ci' environment with localhost:3000

      - name: Show logs on failure
        if: failure()
        run: |
          docker-compose logs app
          docker-compose logs db

      - name: Cleanup
        if: always()
        run: docker-compose down -v
```

**Note:** This requires you to have a `docker-compose.yml` in your repo that:
- Starts PostgreSQL database
- Runs migrations
- Starts the rain-tracker-service
- Exposes port 3000

**Alternative: Skip CI API tests entirely**

Since API tests require the full stack (database + service), consider:
- ✅ Keep API tests for **manual/local testing only** (run in IDE)
- ✅ Focus CI on **unit tests** (repo, service, fetchers)
- ✅ Run full integration tests **locally** before pushing
- ✅ Add API tests to **deployment verification** (smoke tests on staging)

This is often more practical than trying to replicate the full environment in CI.

**Option 2: IntelliJ HTTP Client CLI**

**Installation:**

```bash
# macOS (Homebrew)
brew install ijhttp

# Linux/macOS (ZIP archive)
curl -f -L -o ijhttp.zip "https://jb.gg/ijhttp/latest"
unzip ijhttp.zip
./ijhttp --version

# Docker (any platform)
docker pull jetbrains/intellij-http-client
```

**Run tests:**

```bash
# Using installed CLI
ijhttp http/api-tests.http --env dev --env-file http/http-client.env.json

# Using Docker
docker run --rm -v $PWD:/workdir jetbrains/intellij-http-client \
  http/api-tests.http --env dev --env-file http/http-client.env.json
```

**Requirements:**
- ZIP/Homebrew: Requires JDK 17 or newer
- Docker: No additional requirements
- No IntelliJ IDEA license required

---

### 6. Manual Testing: Scheduler Jobs

**What to test**: Verify schedulers run correctly and insert data

**Instead of integration tests**, use the CLI commands for manual verification:

```bash
# Test reading fetch works
cargo run -- fetch-readings

# Verify in database
psql $DATABASE_URL -c "SELECT COUNT(*) FROM rain_readings;"

# Test gauge list fetch works
cargo run -- fetch-gauge-list

# Verify in database
psql $DATABASE_URL -c "SELECT COUNT(*) FROM gauge_summaries;"
```

**Why skip scheduler integration tests:**
- Scheduler logic is simple (just calls fetcher + repository)
- Fetchers have their own unit tests (parsing)
- Repositories have their own unit tests (DB operations)
- CLI commands provide easy manual verification
- Avoids complex mock server setup (`mockito`)

**If you really need automated scheduler tests:**
- Use the CLI commands in a shell script
- Run against a test database
- Much simpler than mocking HTTP servers

---

### Test Coverage Goals

- **Repository Layer**: 80%+ coverage (unit tests with sqlx::test)
- **Service Layer**: 90%+ coverage (unit tests with mocking)
- **Fetchers**: 85%+ coverage (unit tests for parsing logic)
- **API Validation**: Test PaginationParams and other validation logic

**Not covered by unit tests (tested manually):**
- API endpoints (use HTTP Client instead)
- Scheduler execution (use CLI commands instead)

---

### Running Tests

**Run all Rust unit tests:**
```bash
cargo test
```

**Run specific test module:**
```bash
# Test a specific service
cargo test reading_service

# Test a specific repository
cargo test reading_repository

# Test a specific fetcher
cargo test gauge_list_fetcher
```

**Run with coverage:**
```bash
cargo tarpaulin --out Html
```

**Test API endpoints (JetBrains HTTP Client):**
1. Start server: `cargo run`
2. Open `http/api-tests.http` in RustRover
3. Run all tests or individual requests

**Test scheduler jobs (CLI commands):**
```bash
# Test reading fetch
cargo run -- fetch-readings

# Test gauge list fetch
cargo run -- fetch-gauge-list
```

---

## Parsing Implementation Details

### Text File Format Challenges

The ev_rain.txt file appears to use **whitespace-delimited columns** which can be tricky because:
- Gauge names and locations contain spaces
- Fixed-width columns may not align perfectly
- "General Location" field is the remainder after other fields

### Recommended Parsing Approach

**Strategy 1: Split on multiple spaces (2+ spaces as delimiter)**
```rust
fn parse_gauge_line(line: &str) -> Result<GaugeSummary, GaugeListFetchError> {
    // Split on 2+ spaces to separate fields
    let parts: Vec<&str> = line.split("  ").filter(|s| !s.is_empty()).collect();

    if parts.len() < 8 {
        return Err(GaugeListFetchError::ParseError);
    }

    // Parse each field
    let gauge_name = parts[0].trim();
    let city_town = parts[1].trim();
    let station_id = parts[2].trim();
    let elevation_ft = parts[3].trim().parse::<i32>().ok();
    let rainfall_6h = parts[4].trim().parse::<f64>().ok();
    let rainfall_24h = parts[5].trim().parse::<f64>().ok();
    let msp_zone = parts[6].trim();
    let general_location = parts[7..].join(" ").trim();  // Remainder of line

    Ok(GaugeSummary {
        station_id: station_id.to_string(),
        gauge_name: gauge_name.to_string(),
        city_town: Some(city_town.to_string()),
        elevation_ft,
        rainfall_past_6h_inches: rainfall_6h,
        rainfall_past_24h_inches: rainfall_24h,
        msp_forecast_zone: Some(msp_zone.to_string()),
        general_location: Some(general_location.to_string()),
    })
}
```

**Strategy 2: Use regex with capture groups**
```rust
// Define regex pattern for each field
let re = Regex::new(r"^(.+?)\s{2,}(.+?)\s{2,}(\d+)\s+(\d+)\s+([\d.]+)\s+([\d.]+)\s+(\S+)\s+(.+)$")?;
```

**Recommendation:** Start with Strategy 1 (split on multiple spaces), fallback to Strategy 2 if needed.

### Header Detection

Skip lines until we find data rows:
```rust
fn is_header_line(line: &str) -> bool {
    line.is_empty()
        || line.contains("Precipitation")
        || line.contains("Gauge Name")
        || line.contains("---")
}
```

---

## Testing Strategy

### Unit Tests

1. **gauge_list_fetcher.rs**
   - Test parsing valid gauge line
   - Test handling malformed data
   - Test skipping header lines
   - Test parsing with missing fields (e.g., no location)

2. **db.rs gauge methods**
   - Test upsert creates new record
   - Test upsert updates existing record
   - Test get_all_gauges returns ordered results
   - Test get_gauge_by_id finds correct record
   - Test get_gauge_by_id returns None for missing ID

### Integration Tests

1. **Fetch gauge list from real URL**
   - Verify it returns data
   - Verify parsing doesn't crash
   - Verify we get expected number of gauges (~100-200)

2. **API endpoint tests**
   - GET /api/v1/gauges returns 200
   - GET /api/v1/gauges/{valid_id} returns 200
   - GET /api/v1/gauges/{invalid_id} returns 404
   - Verify JSON structure matches schema

### Manual Testing

1. Run migrations
2. Start service
3. Wait for first gauge list scrape (or trigger manually)
4. Query `/api/v1/gauges` and verify data
5. Query `/api/v1/gauges/59700` (our known gauge)
6. Verify `last_scraped_at` updates on subsequent scrapes

---

## Monitoring and Observability

### Logging

Add structured logging at key points:
- When gauge list fetch starts
- Number of gauges parsed
- Number of gauges upserted
- Any parsing errors (with line content)
- HTTP errors fetching the file

### Metrics to Track (Future)

- Total gauges in system
- Age of last scrape (for alerting)
- Parse failure rate
- API endpoint latency

---

## Security Considerations

1. **Input Validation**
   - Validate station_id format in API (alphanumeric, max length)
   - Sanitize parsed data before DB insert
   - Handle maliciously crafted text file (e.g., extremely long lines)

2. **Rate Limiting**
   - Don't scrape gauge list too frequently (respect county servers)
   - Default to 60 minutes (hourly)

3. **Error Handling**
   - Don't expose internal errors in API responses
   - Log detailed errors server-side only

---

## Open Questions

1. **Scrape Frequency**
   - Proposed: 60 minutes (hourly)
   - Question: Is this frequent enough? The file shows 6-hour and 24-hour totals, so hourly seems reasonable.

2. **Gauge Discovery**
   - When new gauges appear in the list, should we automatically start scraping their individual readings?
   - Out of scope for this implementation, but worth noting for future

3. **Historical Data**
   - Should we track historical snapshots of gauge summaries?
   - Current plan: Just store latest state
   - Future enhancement: Time-series table for gauge summary history

4. **Data Staleness**
   - How should API indicate if data is stale (e.g., scrape failed for 24+ hours)?
   - Could add a health endpoint: `/api/v1/gauges/health`

---

## Success Criteria

### Phase 1: Refactoring
- ✅ Database layer refactored to repository pattern (src/db/ directory)
- ✅ `ReadingRepository` created with all existing functionality
- ✅ `GaugeRepository` created with gauge operations
- ✅ AppState updated to use individual repositories
- ✅ All existing functionality works after refactoring (no regressions)
- ✅ All existing tests pass after refactoring
- ✅ Unified `FetchError` type shared between both fetchers

### Phase 2: New Features
- ✅ New `gauge_summaries` table created and migrated
- ✅ Gauge list scraping runs every 60 minutes (configurable)
- ✅ Text file parsing correctly extracts all gauge fields
- ✅ API endpoint `/api/v1/gauges` returns paginated list with proper metadata
- ✅ Pagination query parameters work correctly (page, page_size)
- ✅ Pagination validation (page_size max 100, page >= 1)
- ✅ API endpoint `/api/v1/gauges/{station_id}` returns single gauge
- ✅ Response includes `last_scraped_at` timestamp

### Phase 3: Integration
- ✅ Both schedulers run concurrently without conflicts
- ✅ Existing individual gauge reading scraping continues to work unchanged
- ✅ Proper error handling for parsing and fetch failures
- ✅ Tests pass for repository pattern, parsing, pagination, and API endpoints
- ✅ Code is maintainable and follows repository pattern best practices

---

## Future Enhancements (Out of Scope)

1. **Auto-discovery**: When new gauges appear in list, automatically start scraping their individual readings
2. **Gauge comparison API**: Compare rainfall across multiple gauges, find max/min
3. **Filtering**: Filter gauges by city, rainfall thresholds, elevation range
4. **Historical tracking**: Store snapshots of gauge summaries over time
5. **Alerting**: Notify when gauge list hasn't been updated in X hours
6. **Search**: Full-text search on gauge name, location, city
7. **Geospatial**: Add lat/lon coordinates, enable radius searches
8. **Dashboard**: Web UI showing all gauges on a map

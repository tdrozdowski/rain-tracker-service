# Phase 2: On-Demand FOPR Import Architecture

## Implementation Status

**Status:** ✅ **FULLY COMPLETE** - 2025-10-30

### Completed Work - 2025-10-30

#### Phase 2 Original Tasks (All Complete)
1. ✅ **Task 1: Widen lat/long validation** (Commit 76ac542)
   - Expanded from Maricopa County (32.0-34.0°N, -113.0 to -111.0°W) to Arizona state bounds
   - New range: 31.0-37.5°N, -115.0 to -108.5°W (with buffer for partnership gauges)
   - File: `src/fopr/metadata_parser.rs:310-349`

2. ✅ **Task 2: Move database code from service to repository** (Commit 58f2d83)
   - Created `ReadingRepository::bulk_insert_historical_readings()`
   - Refactored `FoprImportService::insert_readings_bulk()` to use repository
   - Services now contain only business logic, repositories only data access
   - Files: `src/db/reading_repository.rs`, `src/services/fopr_import_service.rs`

3. ✅ **Task 3: Audit repositories for business logic** (Commit 1665add)
   - Removed water year SQL logic from `MonthlyRainfallRepository`
   - Removed date boundary calculations from repositories
   - Removed retry policy (exponential backoff) from `FoprImportJobRepository`
   - Removed error history construction from repository layer
   - Files: `src/db/monthly_rainfall_repository.rs`, `src/db/fopr_import_job_repository.rs`,
     `src/services/reading_service.rs`, `src/services/fopr_import_service.rs`,
     `src/workers/fopr_import_worker.rs`, `src/scheduler.rs`,
     `src/bin/historical_import.rs`, `tests/integration_test.rs`

#### Production Bug Fixes (Discovered & Fixed)
4. ✅ **Fix: Widen station_id column** (Commit a4896be)
   - **Error**: `"value too long for type character varying(20)"`
   - **Cause**: Partnership gauges have station IDs > 20 characters
   - **Fix**: Migration 20250110000000 widens VARCHAR(20) → VARCHAR(50)
   - **Impact**: 4 tables updated (rain_readings, gauge_summaries, gauges, monthly_rainfall_summary)

5. ✅ **Fix: Widen elevation validation** (Commit 3ea953e)
   - **Error**: `"Elevation 5205 outside reasonable range (500 - 4000 ft)"`
   - **Cause**: Validation too restrictive (Maricopa County only)
   - **Fix**: Widened range from 500-4000 ft → 0-13,000 ft (Arizona state range)
   - **Impact**: Allows northern Arizona & partnership gauges at higher elevations

### Refinement Task
6. ✅ **COMPLETE: Replaced hand-coded retry logic with backon library**
   - **Before**: Hard-coded match statement with 5min, 15min, 45min delays
   - **After**: Using `backon` crate v1.6.0 with `ExponentialBuilder`
   - **Benefits**: Industry-standard backoff algorithm, built-in jitter, configurable
   - **Scope**: Worker layer only (database queue pattern unchanged)
   - **File**: `src/workers/fopr_import_worker.rs:94-112`

### Deployment Status
✅ **Ready for Production**
- All core functionality complete and tested
- Database migrations ready (20250110000000_widen_station_id.sql)
- Build passes: `cargo build`, `cargo clippy -- -D warnings`, `cargo test`
- When deployed:
  - Gauge list scheduler discovers new gauges every 60 minutes
  - FOPR import worker processes jobs every 30 seconds with retry
  - New gauges automatically import complete historical data
  - Partnership gauges outside Maricopa County fully supported

---

## Problem Statement

**Current Situation:**
- Gauge list scraper discovers active gauges from MCFCD website
- Some gauges may be new (not in our `gauges` table)
- `rain_readings` has FK constraint to `gauges.station_id`
- **We cannot insert readings for a gauge until gauge metadata exists**

**Original Plan (Phase 1):**
- Import all historical data from water year Excel files (`pcp_WY_YYYY.xlsx`)
- Files contain 350+ gauges, most of which may not be active/relevant
- Massive bulk import of potentially unnecessary data

**Better Approach (Phase 2):**
- Use FOPR files on-demand when we discover a new gauge
- FOPR files contain both gauge metadata AND complete historical data
- Only import what we need, when we need it
- Automatic backfill when encountering new gauges

## Key Insight

**FOPR files are self-contained:**
- `Meta_Stats` sheet → Gauge metadata (location, elevation, stats)
- Year sheets (2024, 2023, etc.) → Complete daily historical data
- One FOPR file = everything we need for a gauge

**Gauge discovery flow:**
```
Gauge List Scraper runs
    ↓
Finds gauge 59700 on MCFCD website (with summary data)
    ↓
Checks: Does gauge 59700 exist in our DB?
    ├─ YES → Continue processing readings normally
    └─ NO → Queue FOPR import job for gauge 59700 + store summary data
             ↓
             Worker downloads FOPR_59700.xlsx
             ↓
             Imports gauge metadata → gauges table
             ↓
             Imports historical readings → rain_readings table
             ↓
             Inserts stored summary → gauge_summaries table
             ↓
             Gauge is immediately complete (no 60min wait)!
```

## Architecture Overview

### Components (Layered Architecture)

```
┌─────────────────────────────────────────────────────────────┐
│  Schedulers (main.rs) - Coordination Layer                  │
│  ├─ Reading Scheduler (15 min)                              │
│  │  └─ Calls ReadingService                                 │
│  ├─ Gauge List Scheduler (60 min)                           │
│  │  └─ Calls GaugeService                                   │
│  └─ FOPR Import Worker (30 sec poll)                        │
│     └─ Calls FoprImportService                              │
└──────────┬──────────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────┐
│  Services - Business Logic Layer                            │
│  ├─ ReadingService (existing)                               │
│  ├─ GaugeService (enhanced with new method)                 │
│  │  └─ handle_new_gauge_discovery()                         │
│  └─ FoprImportService (NEW)                                 │
│     ├─ process_next_import_job()                            │
│     ├─ import_fopr_for_gauge()                              │
│     ├─ download_fopr_file()                                 │
│     └─ Coordinates: GaugeRepo, ReadingRepo, JobRepo         │
└──────────┬──────────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────┐
│  Repositories - Data Access Layer                           │
│  ├─ ReadingRepository (existing)                            │
│  ├─ GaugeRepository (existing)                              │
│  └─ FoprImportJobRepository (NEW)                           │
│     ├─ claim_next_job() - Atomic get + mark in_progress    │
│     ├─ mark_completed()                                     │
│     └─ mark_failed()                                        │
└──────────┬──────────────────────────────────────────────────┘
           │
           ▼
┌─────────────────────────────────────────────────────────────┐
│  PostgreSQL Database                                         │
│  ├─ fopr_import_jobs (job queue)                            │
│  ├─ gauges (metadata)                                       │
│  ├─ gauge_summaries (live data)                             │
│  └─ rain_readings (historical + live)                       │
└─────────────────────────────────────────────────────────────┘
```

**Key Architecture Principles:**
- ✅ **Schedulers** coordinate and delegate (no business logic)
- ✅ **Services** contain business logic (workflow orchestration, decisions)
- ✅ **Repositories** contain data access only (SQL, no business decisions)
- ✅ **Atomic operations** prevent race conditions (claim_next_job)
- ✅ **Separation of concerns** (each layer has clear responsibility)

### Database Schema

```sql
-- New table for tracking FOPR import jobs
CREATE TABLE fopr_import_jobs (
    id SERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL UNIQUE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
        -- 'pending', 'in_progress', 'completed', 'failed'
    priority INT DEFAULT 0,
        -- Higher priority = processed first (for manually triggered imports)

    -- Timestamps
    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    started_at TIMESTAMP,
    completed_at TIMESTAMP,

    -- Error tracking
    error_message TEXT,
        -- Most recent error message (for simple queries)
    error_history JSONB,
        -- Full error history with timestamps: {"errors": [{"attempt": 1, "timestamp": "...", "error": "..."}]}
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMP,

    -- Metadata
    source VARCHAR(50) DEFAULT 'auto_discovery',
        -- 'auto_discovery', 'manual', 'backfill'
    gauge_summary JSONB,
        -- Stores FetchedGauge from scraper to be inserted after FOPR import
        -- This avoids FK violations and ensures immediate gauge completeness
    import_stats JSONB,
        -- {"rows_imported": 1000, "date_range": "2010-01-01 to 2024-12-31"}

    -- Constraints
    CONSTRAINT valid_status CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    CONSTRAINT valid_retry CHECK (retry_count <= max_retries)
);

-- Indexes for efficient queries
CREATE INDEX idx_fopr_jobs_status ON fopr_import_jobs(status);
CREATE INDEX idx_fopr_jobs_next_retry ON fopr_import_jobs(next_retry_at) WHERE status = 'failed';
CREATE INDEX idx_fopr_jobs_priority ON fopr_import_jobs(priority DESC, created_at ASC);

-- Comments for documentation
COMMENT ON COLUMN fopr_import_jobs.gauge_summary
    IS 'Stores gauge summary data from scraper (FetchedGauge) to be inserted after FOPR import completes. Avoids waiting for next scrape cycle.';

-- Existing tables (already implemented)
CREATE TABLE gauges (
    station_id VARCHAR(20) PRIMARY KEY,
    name VARCHAR(255),
    location VARCHAR(255),
    latitude DECIMAL(9,6),
    longitude DECIMAL(9,6),
    elevation DECIMAL(10,2),
    -- ... other metadata from FOPR Meta_Stats sheet
);

CREATE TABLE rain_readings (
    id SERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL REFERENCES gauges(station_id),
    reading_date DATE NOT NULL,
    rainfall_inches DECIMAL(5,2),
    data_source VARCHAR(50) DEFAULT 'live_scrape',
    import_metadata JSONB,
    UNIQUE(station_id, reading_date)
);
```

## Implementation Components

### Architecture Summary

This implementation follows our layered architecture principles:

**Layer Separation:**
- **Repositories** = Data access only (SQL queries, no business logic)
  - `FoprImportJobRepository::claim_next_job()` - Atomic SQL operation
  - `GaugeRepository::upsert_gauge_metadata()` - Data persistence
  - `ReadingRepository::bulk_insert_readings()` - Data persistence

- **Services** = Business logic (workflow orchestration, decisions)
  - `FoprImportService::process_next_import_job()` - Import workflow
  - `FoprImportService::import_fopr_for_gauge()` - Coordinates repos
  - `GaugeService::handle_new_gauge_discovery()` - Discovery logic

- **Workers/Schedulers** = Thin coordination (polling, delegation)
  - `FoprImportWorker` - Polls and delegates to service
  - `start_gauge_list_scheduler()` - Delegates to service

**Key Improvements:**
1. **Atomic Job Claiming**: `claim_next_job()` prevents race conditions with `FOR UPDATE SKIP LOCKED`
2. **Service-Based**: All business logic in services, not scattered across workers/schedulers
3. **Testable**: Services can be unit tested without workers/schedulers
4. **Scalable**: Multiple worker instances can run safely (atomic operations)
5. **Maintainable**: Clear separation of concerns, easy to understand

### 0. Prerequisites - FetchedGauge Serialization

The `FetchedGauge` struct from `gauge_list_fetcher.rs` must implement `Serialize` and `Deserialize` to be stored as JSONB:

```rust
// src/gauge_list_fetcher.rs

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)] // Add Serialize/Deserialize
pub struct GaugeSummary {
    pub station_id: String,
    pub gauge_name: String,
    pub city_town: String,
    pub elevation_ft: Option<f64>,
    pub general_location: Option<String>,
    pub msp_forecast_zone: Option<String>,
    pub rainfall_past_6h_inches: Option<f64>,
    pub rainfall_past_24h_inches: Option<f64>,
}

// If adding new fields in future, use #[serde(default)] to handle old JSONB data:
// #[serde(default)]
// pub new_field: Option<String>,
```

**Important:** Any future changes to `FetchedGauge` schema should use `#[serde(default)]` for new fields to ensure backward compatibility with existing JSONB data in the database.

### 1. Job Queue Repository

```rust
// src/db/fopr_import_job_repository.rs

use sqlx::PgPool;
use chrono::{DateTime, Utc, Duration};

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "varchar", rename_all = "lowercase")]
pub enum JobStatus {
    Pending,
    InProgress,
    Completed,
    Failed,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct FoprImportJob {
    pub id: i32,
    pub station_id: String,
    pub status: JobStatus,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub error_history: Option<serde_json::Value>,
    pub retry_count: i32,
    pub max_retries: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub source: String,
    pub import_stats: Option<serde_json::Value>,
}

pub struct FoprImportJobRepository {
    pool: PgPool,
}

impl FoprImportJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new import job (idempotent - ignores duplicates)
    pub async fn enqueue_job(
        &self,
        station_id: &str,
        source: &str,
        priority: i32,
        gauge_summary: Option<&serde_json::Value>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            INSERT INTO fopr_import_jobs (station_id, source, priority, gauge_summary)
            VALUES ($1, $2, $3, $4)
            ON CONFLICT (station_id) DO NOTHING
            "#,
            station_id,
            source,
            priority,
            gauge_summary,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Atomically claim next job and mark as in_progress
    ///
    /// This prevents race conditions when multiple workers are running.
    /// Uses FOR UPDATE SKIP LOCKED for safe concurrent access.
    /// Returns the claimed job or None if no jobs available.
    pub async fn claim_next_job(&self) -> Result<Option<FoprImportJob>, sqlx::Error> {
        // Atomic operation: find job + mark in_progress in single UPDATE
        let job = sqlx::query_as!(
            FoprImportJob,
            r#"
            UPDATE fopr_import_jobs
            SET status = 'in_progress',
                started_at = NOW()
            WHERE id = (
                SELECT id
                FROM fopr_import_jobs
                WHERE status = 'pending'
                   OR (status = 'failed' AND retry_count < max_retries AND next_retry_at <= NOW())
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING
                id, station_id,
                status AS "status: JobStatus",
                priority, created_at, started_at, completed_at,
                error_message, error_history, retry_count, max_retries, next_retry_at,
                source, gauge_summary, import_stats
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    /// Mark job as completed
    pub async fn mark_completed(
        &self,
        job_id: i32,
        stats: &serde_json::Value,
    ) -> Result<(), sqlx::Error> {
        sqlx::query!(
            r#"
            UPDATE fopr_import_jobs
            SET status = 'completed',
                completed_at = NOW(),
                import_stats = $2
            WHERE id = $1
            "#,
            job_id,
            stats,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Mark job as failed (with retry logic and error history tracking)
    pub async fn mark_failed(
        &self,
        job_id: i32,
        error: &str,
    ) -> Result<(), sqlx::Error> {
        // Exponential backoff: 5min, 15min, 45min
        // Also append error to history with timestamp and attempt number
        sqlx::query!(
            r#"
            UPDATE fopr_import_jobs
            SET status = 'failed',
                error_message = $2,
                retry_count = retry_count + 1,
                next_retry_at = NOW() + INTERVAL '5 minutes' * POWER(3, retry_count),
                error_history = COALESCE(error_history, '{"errors": []}'::jsonb) ||
                    jsonb_build_object(
                        'errors',
                        COALESCE(error_history->'errors', '[]'::jsonb) ||
                        jsonb_build_array(
                            jsonb_build_object(
                                'attempt', retry_count + 1,
                                'timestamp', NOW(),
                                'error', $2
                            )
                        )
                    )
            WHERE id = $1
            "#,
            job_id,
            error,
        )
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// Check if gauge needs FOPR import
    pub async fn needs_import(&self, station_id: &str) -> Result<bool, sqlx::Error> {
        let result = sqlx::query!(
            r#"
            SELECT EXISTS(
                SELECT 1 FROM gauges WHERE station_id = $1
            ) AS gauge_exists,
            EXISTS(
                SELECT 1 FROM fopr_import_jobs
                WHERE station_id = $1
                AND status IN ('pending', 'in_progress')
            ) AS job_exists
            "#,
            station_id,
        )
        .fetch_one(&self.pool)
        .await?;

        // Needs import if gauge doesn't exist AND no job queued
        Ok(!result.gauge_exists.unwrap_or(false) && !result.job_exists.unwrap_or(false))
    }
}
```

### 2. FOPR Import Service (Business Logic)

```rust
// src/services/fopr_import_service.rs

use std::path::Path;
use sqlx::PgPool;
use crate::db::{FoprImportJobRepository, GaugeRepository, ReadingRepository};
use crate::fopr::{MetadataParser, DailyDataParser};
use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;

pub struct FoprImportService {
    job_repo: FoprImportJobRepository,
    gauge_repo: GaugeRepository,
    reading_repo: ReadingRepository,
}

impl FoprImportService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            job_repo: FoprImportJobRepository::new(pool.clone()),
            gauge_repo: GaugeRepository::new(pool.clone()),
            reading_repo: ReadingRepository::new(pool),
        }
    }

    /// Process next available import job (if any)
    ///
    /// This is the main entry point called by the worker.
    /// Returns true if a job was processed, false if no jobs available.
    pub async fn process_next_import_job(&self) -> Result<bool, Box<dyn std::error::Error>> {
        // Atomically claim next job
        let Some(job) = self.job_repo.claim_next_job().await? else {
            return Ok(false); // No jobs available
        };

        info!("Processing FOPR import for gauge {}", job.station_id);

        // Process the import
        match self.import_fopr_for_gauge(&job).await {
            Ok(stats) => {
                info!(
                    "FOPR import completed for gauge {}: {} readings imported",
                    job.station_id, stats["rows_imported"]
                );

                let stats_json = serde_json::to_value(stats)?;
                self.job_repo.mark_completed(job.id, &stats_json).await?;
                Ok(true)
            }
            Err(e) => {
                error!("FOPR import failed for gauge {}: {}", job.station_id, e);

                let error_msg = format!("{:?}", e);
                self.job_repo.mark_failed(job.id, &error_msg).await?;

                // Still return Ok(true) - we processed a job, it just failed
                Ok(true)
            }
        }
    }

    /// Import FOPR file for a specific gauge
    ///
    /// Business logic for the complete import workflow:
    /// 1. Download FOPR file
    /// 2. Parse gauge metadata
    /// 3. Parse historical readings
    /// 4. Insert gauge metadata
    /// 5. Insert historical readings
    /// 6. Insert gauge summary (if available)
    async fn import_fopr_for_gauge(
        &self,
        job: &FoprImportJob,
    ) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
        let station_id = &job.station_id;

        // 1. Download FOPR file
        let temp_file = self.download_fopr_file(station_id).await?;

        // 2. Parse metadata (in blocking task)
        let temp_file_clone = temp_file.clone();
        let gauge_metadata = tokio::task::spawn_blocking(move || {
            MetadataParser::parse(&temp_file_clone)
        })
        .await??;

        // 3. Insert gauge metadata
        info!("Upserting gauge metadata for {}", station_id);
        self.gauge_repo.upsert_gauge_metadata(&gauge_metadata).await?;

        // 4. Parse historical readings (in blocking task)
        let temp_file_clone = temp_file.clone();
        let station_id_clone = station_id.to_string();
        let readings = tokio::task::spawn_blocking(move || {
            DailyDataParser::parse_all_years(&temp_file_clone, &station_id_clone)
        })
        .await??;

        // 5. Bulk insert readings
        info!("Inserting {} historical readings for {}", readings.len(), station_id);
        let rows_inserted = self.reading_repo.bulk_insert_readings(&readings).await?;

        // 6. Insert gauge summary if we have it
        if let Some(summary_json) = &job.gauge_summary {
            info!("Inserting gauge summary for {}", station_id);

            // Deserialize the stored summary
            let summary: FetchedGauge = serde_json::from_value(summary_json.clone())?;

            // Insert into gauge_summaries (now that gauge exists in gauges table)
            self.gauge_repo.upsert_summaries(&[summary]).await?;

            info!("Successfully inserted gauge summary for {}", station_id);
        } else {
            warn!("No gauge summary stored for {} - will be populated on next scrape", station_id);
        }

        // 7. Clean up temp file
        tokio::fs::remove_file(&temp_file).await?;

        // 8. Return stats
        Ok(serde_json::json!({
            "rows_imported": rows_inserted,
            "total_rows": readings.len(),
            "date_range": format!(
                "{} to {}",
                readings.iter().map(|r| r.reading_date).min().unwrap_or_default(),
                readings.iter().map(|r| r.reading_date).max().unwrap_or_default()
            ),
        }))
    }

    /// Download FOPR file from MCFCD website
    async fn download_fopr_file(
        &self,
        station_id: &str,
    ) -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
        let url = format!(
            "https://alert.fcd.maricopa.gov/alert/Rain/FOPR/FOPR_{}.xlsx",
            station_id
        );

        info!("Downloading FOPR file: {}", url);
        let response = reqwest::get(&url).await?;

        if !response.status().is_success() {
            return Err(format!("Failed to download FOPR: HTTP {}", response.status()).into());
        }

        let bytes = response.bytes().await?;
        let temp_file = std::env::temp_dir().join(format!("fopr_{}.xlsx", station_id));
        tokio::fs::write(&temp_file, &bytes).await?;

        Ok(temp_file)
    }
}
```

### 3. FOPR Import Worker (Thin Delegation Layer)

```rust
// src/workers/fopr_import_worker.rs

use std::sync::Arc;
use tokio::time::{interval, Duration};
use crate::services::FoprImportService;

pub struct FoprImportWorker {
    service: Arc<FoprImportService>,
    poll_interval: Duration,
}

impl FoprImportWorker {
    pub fn new(service: Arc<FoprImportService>, poll_interval_secs: u64) -> Self {
        Self {
            service,
            poll_interval: Duration::from_secs(poll_interval_secs),
        }
    }

    /// Start the worker (runs indefinitely)
    ///
    /// This is a thin coordination layer that just polls and delegates to the service.
    /// All business logic lives in FoprImportService.
    pub async fn run(self: Arc<Self>) {
        let mut ticker = interval(self.poll_interval);

        info!("FOPR import worker started (polling every {:?})", self.poll_interval);

        loop {
            ticker.tick().await;

            // Delegate to service - all business logic happens there
            match self.service.process_next_import_job().await {
                Ok(true) => {
                    debug!("Processed FOPR import job successfully");
                }
                Ok(false) => {
                    trace!("No pending FOPR import jobs");
                }
                Err(e) => {
                    error!("Error processing FOPR import job: {}", e);
                }
            }
        }
    }
}
```

### 4. Gauge Service Enhancement (Business Logic for Discovery)

Add a new method to the existing `GaugeService` to handle new gauge discovery:

```rust
// src/services/gauge_service.rs (add new method to existing service)

impl GaugeService {
    // ... existing methods ...

    /// Handle discovery of a new gauge from scraper
    ///
    /// Business logic: If gauge doesn't exist, enqueue FOPR import job with summary data.
    /// If gauge exists, upsert summary normally.
    pub async fn handle_new_gauge_discovery(
        &self,
        summary: &FetchedGauge,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // Check if gauge exists in gauges table
        let gauge_exists = self.gauge_repo.gauge_exists(&summary.station_id).await?;

        if gauge_exists {
            // Gauge exists - upsert summary normally
            debug!("Gauge {} exists, upserting summary", summary.station_id);
            self.gauge_repo.upsert_summaries(&[summary.clone()]).await?;
        } else {
            // New gauge discovered - check if already queued for import
            let needs_import = self.job_repo.needs_import(&summary.station_id).await?;

            if needs_import {
                info!("New gauge discovered: {} - queueing FOPR import", summary.station_id);

                // Serialize summary to JSONB
                let summary_json = serde_json::to_value(summary)?;

                // Enqueue job with stored summary data
                self.job_repo.enqueue_job(
                    &summary.station_id,
                    "auto_discovery",
                    0, // default priority
                    Some(&summary_json),
                ).await?;
            } else {
                debug!("FOPR import already queued for gauge {}", summary.station_id);
            }
        }

        Ok(())
    }
}
```

**Note:** This requires `GaugeService` to have access to `FoprImportJobRepository`. Update the constructor:

```rust
pub struct GaugeService {
    gauge_repo: GaugeRepository,
    job_repo: FoprImportJobRepository, // NEW
}

impl GaugeService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            gauge_repo: GaugeRepository::new(pool.clone()),
            job_repo: FoprImportJobRepository::new(pool), // NEW
        }
    }

    // ... methods ...
}
```

### 5. Integration with Gauge List Scheduler

Modify the existing gauge list scheduler to delegate to `GaugeService`:

```rust
// src/scheduler.rs (modify existing gauge_list_scheduler)

pub async fn start_gauge_list_scheduler(
    fetcher: GaugeListFetcher,
    gauge_service: GaugeService, // Changed: use service instead of repo
    interval_minutes: u64,
) {
    let mut interval = time::interval(Duration::from_secs(interval_minutes * 60));

    info!("Gauge list scheduler started with {} minute interval", interval_minutes);

    loop {
        interval.tick().await;
        debug!("Gauge list scheduler tick - initiating fetch");

        match fetch_and_store_gauge_list(&fetcher, &gauge_service).await {
            Ok(count) => {
                info!("Successfully processed {} gauge summaries", count);
            }
            Err(e) => {
                error!("Failed to fetch gauge list: {}", e);
            }
        }
    }
}

async fn fetch_and_store_gauge_list(
    fetcher: &GaugeListFetcher,
    gauge_service: &GaugeService, // Changed: use service
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching gauge list");
    let gauges = fetcher.fetch_gauge_list().await?;
    info!("Fetched {} gauges from list", gauges.len());

    let mut processed = 0;

    for summary in gauges {
        // Delegate to service - all business logic handled there
        match gauge_service.handle_new_gauge_discovery(&summary).await {
            Ok(()) => {
                processed += 1;
            }
            Err(e) => {
                error!("Failed to handle gauge {}: {}", summary.station_id, e);
            }
        }
    }

    Ok(processed)
}
```

### 6. Application Initialization Module

Create `src/app.rs` to handle all service initialization and wiring:

```rust
// src/app.rs

use std::sync::Arc;
use axum::Router;
use sqlx::PgPool;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;

use crate::api::{create_router, AppState};
use crate::config::Config;
use crate::db::{MonthlyRainfallRepository, ReadingRepository, GaugeRepository};
use crate::fetcher::RainGaugeFetcher;
use crate::gauge_list_fetcher::GaugeListFetcher;
use crate::scheduler;
use crate::services::{ReadingService, GaugeService, FoprImportService};
use crate::workers::fopr_import_worker::FoprImportWorker;

/// Application holds all initialized services and background tasks
pub struct Application {
    pub server_handle: JoinHandle<()>,
    pub reading_scheduler_handle: JoinHandle<()>,
    pub gauge_list_scheduler_handle: JoinHandle<()>,
    pub fopr_worker_handle: JoinHandle<()>,
}

impl Application {
    /// Initialize and start all application components
    pub async fn build(config: Config, pool: PgPool) -> Result<Self, Box<dyn std::error::Error>> {
        // Initialize services
        let reading_service = Arc::new(ReadingService::new(pool.clone()));
        let gauge_service = Arc::new(GaugeService::new(pool.clone()));
        let fopr_import_service = Arc::new(FoprImportService::new(pool.clone()));

        // Initialize repositories for schedulers
        let reading_repo = ReadingRepository::new(pool.clone());
        let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

        // Initialize fetchers
        let reading_fetcher = RainGaugeFetcher::new(config.gauge_url.clone());
        let gauge_list_fetcher = GaugeListFetcher::new(config.gauge_list_url.clone());

        // Spawn reading scheduler
        let reading_scheduler_handle = tokio::spawn({
            let fetcher = reading_fetcher.clone();
            let reading_repo = reading_repo.clone();
            let monthly_repo = monthly_repo.clone();
            let interval = config.fetch_interval_minutes;
            async move {
                scheduler::start_fetch_scheduler(
                    fetcher,
                    reading_repo,
                    monthly_repo,
                    interval,
                ).await;
            }
        });

        // Spawn gauge list scheduler
        let gauge_list_scheduler_handle = tokio::spawn({
            let fetcher = gauge_list_fetcher.clone();
            let service = gauge_service.clone();
            let interval = config.gauge_list_interval_minutes;
            async move {
                scheduler::start_gauge_list_scheduler(
                    fetcher,
                    service,
                    interval,
                ).await;
            }
        });

        // Spawn FOPR import worker
        let fopr_worker_handle = tokio::spawn({
            let worker = Arc::new(FoprImportWorker::new(
                fopr_import_service.clone(),
                30, // poll every 30 seconds
            ));
            async move {
                worker.run().await;
            }
        });

        // Create API router with services
        let app_state = AppState {
            reading_service,
            gauge_service,
        };
        let app = create_router(app_state).layer(TraceLayer::new_for_http());

        // Start Axum server
        let addr = config.server_addr();
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        info!("Server listening on {}", addr);

        let server_handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .await
                .expect("Server failed");
        });

        Ok(Self {
            server_handle,
            reading_scheduler_handle,
            gauge_list_scheduler_handle,
            fopr_worker_handle,
        })
    }

    /// Wait for all tasks to complete
    pub async fn run_until_stopped(self) -> Result<(), Box<dyn std::error::Error>> {
        tokio::select! {
            _ = self.reading_scheduler_handle => {},
            _ = self.gauge_list_scheduler_handle => {},
            _ = self.fopr_worker_handle => {},
            _ = self.server_handle => {},
        }
        Ok(())
    }
}
```

### 7. Clean Main Entry Point

Now `main.rs` becomes very clean:

```rust
// src/main.rs

mod api;
mod app; // NEW
mod config;
mod db;
mod fetch_error;
mod fetcher;
mod fopr;
mod gauge_list_fetcher;
mod scheduler;
mod services; // NEW
mod workers; // NEW

use app::Application;
use config::Config;
use sqlx::postgres::PgPoolOptions;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "rain_tracker_service=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    // Load configuration
    let config = Config::from_env()?;

    // Create database connection pool
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    // Run database migrations
    sqlx::migrate!().run(&pool).await?;
    info!("Database migrations completed");

    // Build and run application
    let app = Application::build(config, pool).await?;
    app.run_until_stopped().await?;

    Ok(())
}
```

## Migration

```sql
-- migrations/YYYYMMDDHHMMSS_create_fopr_import_jobs.sql

CREATE TABLE fopr_import_jobs (
    id SERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL UNIQUE,
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    priority INT DEFAULT 0,

    created_at TIMESTAMP NOT NULL DEFAULT NOW(),
    started_at TIMESTAMP,
    completed_at TIMESTAMP,

    error_message TEXT,
    error_history JSONB,
    retry_count INT NOT NULL DEFAULT 0,
    max_retries INT NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMP,

    source VARCHAR(50) DEFAULT 'auto_discovery',
    gauge_summary JSONB,
    import_stats JSONB,

    CONSTRAINT valid_status CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    CONSTRAINT valid_retry CHECK (retry_count <= max_retries)
);

CREATE INDEX idx_fopr_jobs_status ON fopr_import_jobs(status);
CREATE INDEX idx_fopr_jobs_next_retry ON fopr_import_jobs(next_retry_at) WHERE status = 'failed';
CREATE INDEX idx_fopr_jobs_priority ON fopr_import_jobs(priority DESC, created_at ASC);

-- Add comments for documentation
COMMENT ON TABLE fopr_import_jobs IS 'Queue for on-demand FOPR historical data imports when new gauges are discovered';
COMMENT ON COLUMN fopr_import_jobs.status IS 'Job status: pending (not started), in_progress (currently processing), completed (success), failed (error, may retry)';
COMMENT ON COLUMN fopr_import_jobs.priority IS 'Higher priority jobs processed first (manual imports > auto-discovery)';
COMMENT ON COLUMN fopr_import_jobs.source IS 'How job was created: auto_discovery (from gauge scraper), manual (CLI/API), backfill (batch operation)';
COMMENT ON COLUMN fopr_import_jobs.error_message IS 'Most recent error message for quick reference (also stored in error_history for full context)';
COMMENT ON COLUMN fopr_import_jobs.error_history IS 'Complete error history as JSONB array: {"errors": [{"attempt": 1, "timestamp": "...", "error": "..."}]}';
COMMENT ON COLUMN fopr_import_jobs.gauge_summary IS 'Stores gauge summary data from scraper (FetchedGauge) to be inserted after FOPR import completes. Avoids waiting for next scrape cycle.';
```

## Configuration

Add to `.env` / `src/config.rs`:

```bash
# FOPR Import Worker Configuration
FOPR_IMPORT_POLL_INTERVAL_SECS=30    # How often to check for pending jobs
FOPR_IMPORT_MAX_RETRIES=3            # Max retry attempts per job
```

## Benefits of This Approach

1. **Zero New Infrastructure**: Uses existing PostgreSQL database
2. **Automatic Discovery**: New gauges automatically get historical data
3. **Non-Blocking**: Import happens asynchronously, doesn't block schedulers
4. **Resilient**: Built-in retry logic with exponential backoff
5. **Trackable**: Full audit trail of import jobs and status
6. **Prioritization**: Can manually trigger high-priority imports
7. **Idempotent**: Safe to queue same gauge multiple times
8. **Efficient**: Only imports data for active/relevant gauges
9. **FK Safe**: Ensures gauge metadata exists before readings import
10. **Immediate Completeness**: Gauge summary stored with job and inserted after FOPR import - no 60-minute wait for next scrape cycle
11. **No Data Loss**: Summary data captured at discovery time, even if gauge goes offline before next scrape
12. **Atomic Operations**: Gauge metadata + historical readings + summary all inserted in one workflow
13. **Error History Tracking**: Full error history with timestamps for debugging retry patterns and distinguishing transient vs permanent failures

## Error History Tracking

### Why Track Error History?

When jobs fail and retry multiple times, understanding the pattern of failures is critical for debugging. Each retry attempt may fail for different reasons:

- Attempt 1: Network timeout (transient)
- Attempt 2: Invalid Excel format (permanent)
- Attempt 3: Database deadlock (transient)

Without history, you only see the last error and lose valuable debugging context.

### Implementation

**Storage:**
```json
{
  "errors": [
    {
      "attempt": 1,
      "timestamp": "2025-10-28T10:15:30.123456Z",
      "error": "Network timeout: connection refused after 30s"
    },
    {
      "attempt": 2,
      "timestamp": "2025-10-28T10:20:45.789012Z",
      "error": "Parse error: Sheet 'Meta_Stats' not found in Excel file"
    }
  ]
}
```

**How it works:**
1. First failure creates `error_history` with one entry
2. Each subsequent failure appends a new entry
3. `error_message` column always shows most recent error (for simple queries)
4. Full history available in `error_history` JSONB (for deep debugging)

**Benefits:**
- Identify patterns (always fails on attempt 2)
- Distinguish transient vs permanent errors
- Track when errors occurred (time-based analysis)
- Better debugging without external log aggregation

### Example Error Analysis Queries

```sql
-- Find jobs that fail consistently with the same error
SELECT
    station_id,
    retry_count,
    COUNT(DISTINCT attempt->>'error') AS unique_errors
FROM fopr_import_jobs,
     jsonb_array_elements(error_history->'errors') AS attempt
WHERE status = 'failed'
GROUP BY station_id, retry_count
HAVING COUNT(DISTINCT attempt->>'error') = 1;  -- Same error every time

-- Find jobs with escalating/changing errors
SELECT
    station_id,
    retry_count,
    array_agg(attempt->>'error' ORDER BY (attempt->>'attempt')::int) AS error_progression
FROM fopr_import_jobs,
     jsonb_array_elements(error_history->'errors') AS attempt
WHERE status = 'failed'
GROUP BY station_id, retry_count;
```

## Edge Cases and Handling

### 1. Manual FOPR Import (No Summary Available)
```bash
# CLI: historical-import fopr --station-id 59700
job_repo.enqueue_job("59700", "manual", 10, None).await?;
```
**Handling:** Worker checks for `None` and skips summary insertion. Summary will be populated on next scrape cycle (max 60 min).

### 2. Summary Data Stale by Import Time
**Scenario:** Gauge summary captured at T, FOPR import completes at T+5min. Summary data is 5 minutes old.

**Handling:** This is acceptable. Next scrape (max 60 min later) will update with fresh data. Having stale data is better than no data.

### 3. FOPR Import Fails, Retry Later
**Scenario:** Download fails, gauge metadata parse error, etc.

**Handling:** Summary data is preserved in job row. All retries use the same stored summary. No need to re-fetch from scraper.

### 4. Gauge Exists But Summary Doesn't
**Scenario:** Gauge imported via manual FOPR import, but no summary in `gauge_summaries` table.

**Handling:** Scheduler will call `upsert_summaries()` on next scrape since `gauge_exists()` returns true. Not an issue.

### 5. FetchedGauge Schema Changes
**Scenario:** Add new field to `FetchedGauge` struct. Old jobs have JSONB without that field.

**Handling:** Serde will deserialize with default values for missing fields (use `#[serde(default)]`). Document this requirement in code.

### 6. Gauge Goes Offline Between Discovery and Import
**Scenario:** Gauge discovered at T, goes offline at T+30s, FOPR import runs at T+1min.

**Handling:**
- Summary data is safely stored in job
- FOPR file download may still succeed (historical data)
- If FOPR download fails (404), job enters retry logic
- Summary will be inserted whenever FOPR eventually succeeds

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[sqlx::test]
    async fn test_enqueue_job_idempotent(pool: PgPool) {
        let repo = FoprImportJobRepository::new(pool);

        // Enqueue twice - should not fail
        repo.enqueue_job("59700", "test", 0).await.unwrap();
        repo.enqueue_job("59700", "test", 0).await.unwrap();

        // Should only have one job
        let count = sqlx::query!("SELECT COUNT(*) as count FROM fopr_import_jobs")
            .fetch_one(&repo.pool)
            .await
            .unwrap();

        assert_eq!(count.count, Some(1));
    }

    #[sqlx::test]
    async fn test_needs_import_logic(pool: PgPool) {
        let repo = FoprImportJobRepository::new(pool);

        // New gauge - should need import
        assert!(repo.needs_import("99999").await.unwrap());

        // Queue job
        repo.enqueue_job("99999", "test", 0, None).await.unwrap();

        // Should NOT need import (job queued)
        assert!(!repo.needs_import("99999").await.unwrap());
    }

    #[sqlx::test]
    async fn test_error_history_tracking(pool: PgPool) {
        let repo = FoprImportJobRepository::new(pool);

        // Create job
        repo.enqueue_job("59700", "test", 0, None).await.unwrap();
        let job = repo.get_next_pending_job().await.unwrap().unwrap();

        // Mark as in progress
        repo.mark_in_progress(job.id).await.unwrap();

        // Fail multiple times with different errors
        repo.mark_failed(job.id, "Network timeout").await.unwrap();
        repo.mark_failed(job.id, "Parse error").await.unwrap();
        repo.mark_failed(job.id, "Database deadlock").await.unwrap();

        // Verify error history
        let updated_job = sqlx::query!(
            r#"SELECT error_message, error_history, retry_count FROM fopr_import_jobs WHERE id = $1"#,
            job.id
        )
        .fetch_one(&repo.pool)
        .await
        .unwrap();

        assert_eq!(updated_job.error_message.unwrap(), "Database deadlock");
        assert_eq!(updated_job.retry_count, 3);

        let history = updated_job.error_history.unwrap();
        let errors = history.get("errors").unwrap().as_array().unwrap();
        assert_eq!(errors.len(), 3);
        assert_eq!(errors[0].get("error").unwrap().as_str().unwrap(), "Network timeout");
        assert_eq!(errors[1].get("error").unwrap().as_str().unwrap(), "Parse error");
        assert_eq!(errors[2].get("error").unwrap().as_str().unwrap(), "Database deadlock");
    }
}
```

### Integration Tests

```rust
#[sqlx::test]
async fn test_full_import_workflow(pool: PgPool) {
    // 1. Enqueue job
    let job_repo = FoprImportJobRepository::new(pool.clone());
    job_repo.enqueue_job("59700", "test", 0).await.unwrap();

    // 2. Get next job
    let job = job_repo.get_next_pending_job().await.unwrap().unwrap();
    assert_eq!(job.station_id, "59700");

    // 3. Mark in progress
    job_repo.mark_in_progress(job.id).await.unwrap();

    // 4. Simulate processing (would actually import here)
    // ...

    // 5. Mark completed
    let stats = serde_json::json!({"rows_imported": 1000});
    job_repo.mark_completed(job.id, &stats).await.unwrap();

    // 6. Verify gauge no longer needs import
    assert!(!job_repo.needs_import("59700").await.unwrap());
}
```

## Manual Operations

### CLI Commands (via existing historical_import binary)

```bash
# Manually trigger FOPR import for specific gauge (no summary - will populate on next scrape)
./target/debug/historical-import fopr --station-id 59700 --enqueue-only

# Import immediately (don't queue)
./target/debug/historical-import fopr --station-id 59700

# Backfill multiple gauges (no summaries)
./target/debug/historical-import fopr --station-ids 59700,12345,67890 --enqueue-only

# Note: Manual imports don't include gauge_summary in job (None)
# This is fine - summary will be populated on next gauge list scrape (max 60 min)
```

### Database Queries

```sql
-- View pending jobs
SELECT station_id, created_at, retry_count
FROM fopr_import_jobs
WHERE status = 'pending'
ORDER BY priority DESC, created_at ASC;

-- View failed jobs with most recent error
SELECT station_id, error_message, retry_count, next_retry_at
FROM fopr_import_jobs
WHERE status = 'failed';

-- View full error history for a specific job
SELECT
    station_id,
    retry_count,
    error_message AS latest_error,
    jsonb_pretty(error_history) AS full_error_history
FROM fopr_import_jobs
WHERE station_id = '59700';

-- View all error attempts for failed jobs
SELECT
    station_id,
    status,
    retry_count,
    error_history->'errors' AS all_errors
FROM fopr_import_jobs
WHERE status = 'failed' AND error_history IS NOT NULL;

-- Extract individual error attempts with details
SELECT
    station_id,
    retry_count,
    attempt->>'attempt' AS attempt_number,
    attempt->>'timestamp' AS error_time,
    attempt->>'error' AS error_message
FROM fopr_import_jobs,
     jsonb_array_elements(error_history->'errors') AS attempt
WHERE status = 'failed'
ORDER BY station_id, (attempt->>'attempt')::int;

-- Manually retry failed job (reset retry counter)
UPDATE fopr_import_jobs
SET status = 'pending', retry_count = 0, error_message = NULL
WHERE station_id = '59700';

-- View import history
SELECT station_id, status, created_at, completed_at, import_stats
FROM fopr_import_jobs
ORDER BY completed_at DESC
LIMIT 20;

-- View jobs with stored gauge summaries
SELECT
    station_id,
    status,
    gauge_summary->>'gauge_name' AS gauge_name,
    gauge_summary->>'city_town' AS city,
    (gauge_summary->>'rainfall_past_24h_inches')::DECIMAL AS rainfall_24h,
    created_at
FROM fopr_import_jobs
WHERE gauge_summary IS NOT NULL
ORDER BY created_at DESC;

-- Check which gauges need backfill
SELECT station_id
FROM gauges
WHERE station_id NOT IN (
    SELECT station_id FROM fopr_import_jobs WHERE status = 'completed'
);
```

## Future Enhancements

1. **API Endpoint**: `/api/v1/admin/fopr/import?station_id=59700` to trigger manual imports
2. **Metrics**: Track import success/failure rates, processing time
3. **Notifications**: Alert on repeated failures (Slack, email, etc.)
4. **Dashboard**: Web UI to view job queue status
5. **Batch Operations**: Bulk enqueue all active gauges for backfill
6. **Pause/Resume**: Ability to pause worker during maintenance
7. **Concurrency**: Process multiple jobs in parallel (thread pool)

## Migration from Phase 1

Since Phase 1 FOPR import CLI already exists:

1. **Keep existing CLI**: Still useful for manual/one-off imports
2. **Add new worker**: Handles automatic discovery imports
3. **Share code**: Both use same FOPR parsing logic (`src/fopr/`)
4. **No breaking changes**: Existing tools continue to work

## Rollout Plan

1. **Create migration**: Add `fopr_import_jobs` table
2. **Implement repository**: `FoprImportJobRepository`
3. **Implement worker**: `FoprImportWorker`
4. **Update scheduler**: Modify gauge list scheduler to enqueue jobs
5. **Update main.rs**: Spawn worker task
6. **Test locally**: Simulate new gauge discovery
7. **Deploy**: K8s deployment (service already handles multiple tasks)
8. **Monitor**: Watch logs for successful imports

## Success Metrics

- **Coverage**: Percentage of active gauges with historical data imported
- **Latency**: Time from gauge discovery to FOPR import completion
- **Reliability**: Success rate of import jobs (target: >95%)
- **Backlog**: Number of pending jobs (should stay near zero)
- **Errors**: Track and alert on repeated failures for same gauge

## Comparison to Original Plan

| Aspect | Phase 1 (Water Year) | Phase 2 (On-Demand FOPR) |
|--------|---------------------|--------------------------|
| Data source | `pcp_WY_YYYY.xlsx` | `FOPR_STATIONID.xlsx` |
| Scope | All 350+ gauges | Only active/discovered gauges |
| Trigger | Manual/scheduled | Automatic on discovery |
| Efficiency | Import unused data | Import only what's needed |
| Complexity | Bulk processing | Incremental, queue-based |
| Infrastructure | One-time K8s Job | Continuous worker task |
| FK handling | Must pre-import all gauges | Just-in-time import |
| Summary availability | Must wait for scrape | Immediate (stored with job) |

## Questions to Resolve

1. **Worker concurrency**: Should we process multiple jobs in parallel?
   - Start with sequential, add concurrency if needed

2. **Job expiration**: Should old failed jobs be purged?
   - Keep for audit trail, add `archived_at` column if needed

3. **Priority system**: When would we use different priorities?
   - Manual imports = high priority
   - Auto-discovery = normal priority
   - Backfill batch = low priority

4. **Monitoring**: What metrics should we expose?
   - Prometheus endpoint for job queue depth, success/failure rates

## Conclusion

This on-demand approach is superior to bulk water year imports:

- ✅ More efficient (import only active gauges)
- ✅ Automatic (no manual intervention needed)
- ✅ Resilient (retry logic, non-blocking)
- ✅ Scalable (handles new gauges automatically)
- ✅ Simple (PostgreSQL queue, no new infrastructure)
- ✅ Immediate completeness (gauge summary stored with job, no 60-min wait)
- ✅ No data loss (summary captured at discovery time)

**Key Innovation: Storing Gauge Summary**
By storing the `FetchedGauge` data as JSONB in the import job, we eliminate the need to wait for the next scrape cycle. The gauge is immediately complete (metadata + readings + summary) after FOPR import, providing a better user experience and more atomic operations.

**Next Steps:**
1. Review and approve this plan
2. **Database Layer**:
   - Create migration with `fopr_import_jobs` table (gauge_summary, error_history columns)
   - Run migration on dev/test databases
3. **Repository Layer** (Data Access):
   - Implement `FoprImportJobRepository` with atomic `claim_next_job()`
   - Add `needs_import()` helper method
4. **Service Layer** (Business Logic):
   - Implement `FoprImportService` with import workflow
   - Enhance `GaugeService` with `handle_new_gauge_discovery()`
   - Add `FoprImportJobRepository` to `GaugeService` constructor
5. **Worker Layer** (Coordination):
   - Implement thin `FoprImportWorker` that delegates to service
   - Update `start_gauge_list_scheduler()` to use service
6. **Integration**:
   - Update `main.rs` with proper service initialization
   - Wire up all dependencies
7. **Testing**:
   - Unit test services (import workflow, discovery logic)
   - Unit test repository (atomic operations, error history)
   - Integration test end-to-end (including retry scenarios)
   - Test concurrency (multiple workers claiming jobs)
8. **Deployment**:
   - Deploy and monitor
   - Verify no race conditions in production

# Historical Data Import Plan

## Overview

This plan describes the implementation of a bulk import system for historical rain gauge data from the Maricopa County Flood Control District (MCFCD). The system will support two data formats based on the year cutoff:

- **Pre-2022**: PDF files by month (`pcpMMYY.pdf`)
- **2022+**: Excel files by water year (`pcp_WY_YYYY.xlsx`)

## Data Format Analysis

### PDF Format (Pre-2022)

**File Naming**: `pcpMMYY.pdf` where:
- `MM` = Month (01-12)
- `YY` = Year (two digits)
- Example: `pcp1119.pdf` = November 2019

**URL Pattern**:
```
https://alert.fcd.maricopa.gov/alert/Rain/pcp1119.pdf
```

**Structure**:
- Multi-page document (typically 40+ pages)
- Organized into gauge groups (G001 through G045)
- Each page contains 8 gauge IDs
- Daily precipitation values in inches
- Date format: MM/DD/YY
- Monthly totals at bottom

**Special Cases**:
- Missing data: Underscores (`____`) indicate gauge outages
- Footnotes: Provide details about gauge failures, estimates, or maintenance
- Estimated values: Sometimes noted with footnote markers like `(1)`

**Example Data Layout**:
```
FCD of Maricopa County ALERT System
G001: Rain Gage Group 01

Gage ID    1000    1200    1300    1500    1600    1700    1800    1900
Daily precipitation values in inches

11/30/19   0.04    0.35    0.00    0.04    0.39    0.63    0.00    0.00
11/29/19   0.16    0.59    0.83    1.22    1.50    0.75    0.39    0.24
...
TOTALS:    3.66    2.76    1.93    5.20    7.48    8.27    1.22    0.87
```

### Excel Format (2022+)

**File Naming**: `pcp_WY_YYYY.xlsx` where:
- `WY` = Water Year
- `YYYY` = Four-digit year
- Example: `pcp_WY_2023.xlsx` = Water Year 2023 (Oct 1, 2022 - Sep 30, 2023)

**URL Pattern**:
```
https://alert.fcd.maricopa.gov/alert/Rain/pcp_WY_2023.xlsx
```

**Confirmed Structure** (analyzed from `pcp_WY_2023.xlsx`):

**Workbook Layout**:
- **13 sheets total**: One per month (OCT, NOV, DEC, JAN, FEB, MAR, APR, MAY, JUN, JUL, AUG, SEP) + Annual_Totals
- **Sheet order**: Reverse chronological (SEP → OCT → Annual_Totals)
- **Dimensions**: Approximately A1:MZ49 (362+ columns, ~49 rows per sheet)

**Sheet Structure** (individual month, e.g., OCT):
```
Row 1: Header            "FCD of Maricopa County ALERT System"
Row 2: Column numbers    1, 2, 3, 4, ..., 362
Row 3: Gage IDs          1000, 1200, 1500, 1600, 1700, ...
Row 4-34: Daily data     YYYY-MM-DD | rainfall values (inches)
Row 35: Monthly totals   "Totals:" | sum for each gauge
Row 36: Empty
Row 37+: Footnotes       "Footnotes:" followed by notes
```

**Data Format**:
- **Row 3**: Gauge IDs (5-digit station IDs: 1000, 1200, 1500, etc.)
  - Approximately 350+ gauges across the system
  - All gauges included in single sheet (unlike PDF multi-page format)
- **Rows 4-34**: Daily precipitation readings
  - Column A: ISO date format `YYYY-MM-DD` (e.g., `2022-10-31`, `2022-10-30`)
  - **Dates in reverse order**: Most recent first (Oct 31 → Oct 1)
  - Columns B onward: Rainfall values in inches (decimal format: `0.03937`, `1.14173`)
  - Missing/no rain: `0` (not null or underscore)
- **Row 35**: Monthly totals for each gauge

**Annual_Totals Sheet**:
```
Row 1: Header
Row 2: Column numbers
Row 3: Gage IDs (same as monthly sheets)
Row 5-16: Monthly totals (SEP 2023, AUG 2023, JUL 2023, ..., OCT 2022)
```

**Key Differences from PDF**:
- ✅ **All gauges in single sheet** (PDF splits into 8-gauge pages)
- ✅ **ISO date format** (`YYYY-MM-DD` vs. `MM/DD/YY`)
- ✅ **Reverse chronological order** (newest → oldest)
- ✅ **Decimal precision**: Values like `0.03937` (1mm), `2.6377899999` (very precise)
- ✅ **Zeros for no rain**: `0` instead of blank or underscore
- ✅ **Footnotes at bottom** (similar to PDF)
- ⚠️ **No gauge group labels** (PDF has G001-G045)

**Sample Data Layout**:
```
Row 3:  Gage ID ---> | 1000    | 1200    | 1500    | ... | 89500
Row 4:  2022-10-31   | 0       | 0       | 0       | ... | 0
Row 5:  2022-10-30   | 0       | 0       | 0       | ... | 0
Row 20: 2022-10-15   | 1.18110 | 0.27559 | 0.86614 | ... | 0.23622
Row 35: Totals:      | 2.71653 | 1.06299 | 1.02362 | ... | 0.07874
```

**Sample File**: `plans/pcp_WY_2023.xlsx`

## Database Schema Design

### Recommended Approach: Unified Table (Option 1)

Extend the existing `rain_readings` table to include historical data. This provides:
- ✅ Seamless querying across all time periods
- ✅ Automatic gap-filling for live scraping
- ✅ Consistent schema and queries
- ✅ Reuse of existing aggregation logic
- ✅ Built-in deduplication via unique constraint

### Schema Changes

```sql
-- Migration: Add columns for data source tracking
ALTER TABLE rain_readings
ADD COLUMN IF NOT EXISTS data_source VARCHAR(50) DEFAULT 'live_scrape',
ADD COLUMN IF NOT EXISTS import_metadata JSONB;

-- Index for filtering by source
CREATE INDEX idx_rain_readings_data_source ON rain_readings(data_source);

-- Data source values:
-- 'live_scrape'     - Current real-time scraping
-- 'pdf_MMYY'        - PDF import (e.g., 'pdf_1119')
-- 'excel_WY_YYYY'   - Excel import (e.g., 'excel_WY_2023')

-- import_metadata JSONB examples:
-- {"footnote": "Gage down due to battery failure", "estimated": true}
-- {"outage_start": "2019-11-13T06:00:00", "outage_end": "2019-11-16T12:00:00"}
```

### Alternative Approaches (Not Recommended)

**Option 2: Separate Historical Table**
```sql
CREATE TABLE historical_rain_readings (
    id SERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL,
    reading_date DATE NOT NULL,
    rainfall_inches DECIMAL(5,2),
    data_source VARCHAR(50) NOT NULL,
    import_date TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
    notes TEXT,
    UNIQUE(station_id, reading_date)
);
```
**Cons**: Complex queries when spanning live + historical data, duplicate schema maintenance

**Option 3: Partitioned Table**
```sql
-- Partition by date ranges (current vs historical)
```
**Cons**: Added complexity, premature optimization

## Architecture Overview

```
┌─────────────────────────────────────┐
│  K8s Job / CronJob                  │
│  - Bulk import (one-time)           │
│  - Single water year import         │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  historical_import Binary           │
│  ├─ CLI argument parsing            │
│  ├─ HTTP downloader (reqwest)       │
│  ├─ PDF parser (pdf-extract/lopdf)  │
│  ├─ Excel parser (calamine)         │
│  ├─ Data validator                  │
│  └─ Bulk DB writer (SQLx)           │
└──────────┬──────────────────────────┘
           │
           ▼
┌─────────────────────────────────────┐
│  PostgreSQL                         │
│  rain_readings table                │
│  (existing + new columns)           │
└─────────────────────────────────────┘
```

## Implementation Components

### 1. Project Structure

```
src/
├── bin/
│   └── historical_import.rs       // Main CLI entry point
├── importers.rs                   // Module declaration (public exports)
├── importers/
│   ├── pdf_importer.rs            // PDF parsing logic
│   ├── excel_importer.rs          // Excel parsing logic
│   ├── downloader.rs              // HTTP download from MCFCD
│   └── validator.rs               // Data validation
├── models.rs                      // Module declaration (if needed)
├── models/
│   └── historical_reading.rs      // Data structures
└── db/
    └── historical_repository.rs   // Bulk insert operations

k8s/jobs/
├── historical-bulk-import.yaml        // One-time bulk import job
└── historical-single-year-import.yaml // Single water year import job

scripts/
├── import-bulk.sh                 // Helper: run bulk import
└── import-water-year.sh           // Helper: run single year import

migrations/
└── YYYYMMDDHHMMSS_add_historical_tracking.sql
```

**Modern Rust Module Structure (Rust 2018+)**:
- `src/importers.rs` declares the `importers` module and its submodules
- `src/importers/*.rs` contains the implementation files
- **NO `mod.rs` files** - this is the old Rust 2015 style
- Each `*.rs` file in a directory is declared in the parent module file

Example `src/importers.rs`:
```rust
//! Historical data importers for PDF and Excel formats

pub mod pdf_importer;
pub mod excel_importer;
pub mod downloader;
pub mod validator;

// Re-export commonly used items
pub use excel_importer::ExcelImporter;
pub use pdf_importer::PdfImporter;
pub use downloader::Downloader;
pub use validator::Validator;
```

### 2. Rust Dependencies

Add to `Cargo.toml`:
```toml
[dependencies]
# Existing dependencies: tokio, axum, sqlx, reqwest, chrono, etc.

# PDF parsing
pdf-extract = "0.7"
# OR
lopdf = "0.32"

# Excel parsing for historical data import
# Read-only, pure Rust, works with spawn_blocking()
# Last updated: 2025-09 (actively maintained)
# Repository: https://github.com/tafia/calamine
# Supports: xlsx, xlsb, xls, ods formats
calamine = { version = "0.31", features = ["dates"] }

# CLI
clap = { version = "4.5", features = ["derive"] }

# Progress bars
indicatif = "0.17"

# Async batch processing
futures = "0.3"
tokio-stream = "0.1"
```

**Why calamine 0.31?**
- ✅ **Actively maintained**: Last updated September 2025
- ✅ **Pure Rust**: No C dependencies, safe and portable
- ✅ **Tokio compatible**: Use with `tokio::task::spawn_blocking()`
- ✅ **Read-only**: Perfect for our import-only use case
- ✅ **Chrono integration**: `dates` feature provides seamless date parsing
- ✅ **Performance**: Fast (<1s for 1MB files) and memory-efficient (~100MB)
- ✅ **Multiple formats**: Supports xlsx, xlsb, xls, ods

**Integration approach:**
- Synchronous API is used with `tokio::task::spawn_blocking()` for file I/O
- Parsed data returned to async context for SQLx bulk inserts
- This pattern aligns with our Tokio/Axum/SQLx architecture

### 3. CLI Interface

```bash
# Import all available data (2010-2024)
./historical_import --mode bulk --start-year 2010 --end-year 2024

# Import specific water year (Excel format)
./historical_import --mode single --water-year 2023

# Import specific month (PDF format)
./historical_import --mode single --month 11 --year 2019

# Import from local file (testing)
./historical_import --mode file --path ./plans/pcp_WY_2023.xlsx

# Dry run (validate without inserting)
./historical_import --mode bulk --start-year 2022 --end-year 2024 --dry-run

# Verbose logging
./historical_import --mode single --water-year 2023 --verbose
```

### 4. K8s Job Manifests

**Approach**: Use `generateName` for automatic unique job names, allowing multiple runs without conflicts.

#### Bulk Import Job (One-Time)

```yaml
# k8s/jobs/historical-bulk-import.yaml
apiVersion: batch/v1
kind: Job
metadata:
  generateName: historical-bulk-import-
  labels:
    app: rain-tracker
    job-type: historical-bulk-import
spec:
  ttlSecondsAfterFinished: 86400  # Clean up after 24 hours
  backoffLimit: 1  # Only retry once if fails
  template:
    metadata:
      labels:
        app: rain-tracker
        job-type: historical-bulk-import
    spec:
      restartPolicy: OnFailure
      containers:
      - name: importer
        image: ghcr.io/your-org/rain-tracker-service:latest
        command: ["/app/historical_import"]
        args:
          - "--mode=bulk"
          - "--start-year=2010"
          - "--end-year=2024"
          - "--verbose"
        env:
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secrets
              key: DATABASE_URL
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "512Mi"
            cpu: "500m"
          limits:
            memory: "2Gi"
            cpu: "2000m"
```

**Usage:**
```bash
# Run the bulk import
kubectl create -f k8s/jobs/historical-bulk-import.yaml

# Watch logs in real-time
kubectl logs -f -l job-type=historical-bulk-import

# Check job status
kubectl get jobs -l job-type=historical-bulk-import

# Get detailed info
kubectl describe job <job-name>
```

#### Single Water Year Job (For Corrections/Updates)

```yaml
# k8s/jobs/historical-single-year-import.yaml
apiVersion: batch/v1
kind: Job
metadata:
  generateName: historical-wy-import-
  labels:
    app: rain-tracker
    job-type: historical-single-year
spec:
  ttlSecondsAfterFinished: 86400
  backoffLimit: 1
  template:
    metadata:
      labels:
        app: rain-tracker
        job-type: historical-single-year
    spec:
      restartPolicy: OnFailure
      containers:
      - name: importer
        image: ghcr.io/your-org/rain-tracker-service:latest
        command: ["/app/historical_import"]
        args:
          - "--mode=single"
          - "--water-year=$(WATER_YEAR)"
          - "--verbose"
        env:
        - name: WATER_YEAR
          value: "2023"  # Change this value before applying
        - name: DATABASE_URL
          valueFrom:
            secretKeyRef:
              name: db-secrets
              key: DATABASE_URL
        - name: RUST_LOG
          value: "info"
        resources:
          requests:
            memory: "256Mi"
            cpu: "250m"
          limits:
            memory: "1Gi"
            cpu: "1000m"
```

**Usage:**
```bash
# Import specific water year (edit inline with sed)
cat k8s/jobs/historical-single-year-import.yaml | \
  sed 's/value: "2023"/value: "2024"/' | \
  kubectl create -f -

# Watch logs
kubectl logs -f -l job-type=historical-single-year

# Check status
kubectl get jobs -l job-type=historical-single-year
```

#### Optional Helper Scripts

For convenience, create shell scripts to simplify job execution:

```bash
# scripts/import-bulk.sh
#!/bin/bash
set -e

echo "🚀 Starting bulk historical data import (2010-2024)..."
kubectl create -f k8s/jobs/historical-bulk-import.yaml

echo ""
echo "✅ Job created. Monitor with:"
echo "   kubectl logs -f -l job-type=historical-bulk-import"
echo ""
echo "Check status with:"
echo "   kubectl get jobs -l job-type=historical-bulk-import"
```

```bash
# scripts/import-water-year.sh
#!/bin/bash
set -e

WATER_YEAR=${1:-2023}

echo "🚀 Starting import for water year $WATER_YEAR..."

cat k8s/jobs/historical-single-year-import.yaml | \
  sed "s/value: \"2023\"/value: \"$WATER_YEAR\"/" | \
  kubectl create -f -

echo ""
echo "✅ Job created for water year $WATER_YEAR"
echo ""
echo "Monitor with:"
echo "   kubectl logs -f -l job-type=historical-single-year"
echo ""
echo "Check status with:"
echo "   kubectl get jobs -l job-type=historical-single-year"
```

**Make scripts executable:**
```bash
chmod +x scripts/import-bulk.sh
chmod +x scripts/import-water-year.sh
```

**Usage:**
```bash
# Run bulk import
./scripts/import-bulk.sh

# Import specific water year
./scripts/import-water-year.sh 2024
./scripts/import-water-year.sh 2022
```

#### Troubleshooting Jobs

```bash
# View all historical import jobs
kubectl get jobs -l app=rain-tracker

# Get pods for a specific job
kubectl get pods -l job-type=historical-bulk-import

# View pod logs if job failed
kubectl logs <pod-name>

# Describe job for detailed status
kubectl describe job <job-name>

# Delete old completed jobs manually
kubectl delete job -l job-type=historical-bulk-import --field-selector status.successful=1

# Delete all historical import jobs
kubectl delete jobs -l app=rain-tracker
```

**Why `generateName`?**
- ✅ Avoids name conflicts - can run same job multiple times
- ✅ K8s automatically appends random suffix (e.g., `historical-bulk-import-xj7k2`)
- ✅ No need to manually edit job names
- ✅ Job history preserved for troubleshooting

### 5. Data Processing Pipeline

#### PDF Import Flow

```
1. Download PDF file from MCFCD
   ├─ URL: https://alert.fcd.maricopa.gov/alert/Rain/pcpMMYY.pdf
   └─ Save to temp file or process in memory

2. Parse PDF structure
   ├─ Iterate through pages
   ├─ Identify gauge groups (G001-G045)
   ├─ Extract gauge IDs from header row
   └─ Parse date rows with precipitation values

3. Extract data
   ├─ Date column → chrono::NaiveDate
   ├─ Gauge columns → Option<f64>
   ├─ Handle missing data (underscores → None)
   └─ Extract footnotes → import_metadata

4. Validate data
   ├─ Date within expected month/year
   ├─ Rainfall values 0.00-20.00 inches
   ├─ Station ID exists in gauge registry
   └─ No future dates

5. Bulk insert
   ├─ Batch 1000 rows at a time
   ├─ ON CONFLICT (station_id, reading_date) DO NOTHING
   ├─ Track: inserted, skipped (duplicates), errors
   └─ Commit transaction per file
```

#### Excel Import Flow

```
1. Download Excel file from MCFCD
   ├─ URL: https://alert.fcd.maricopa.gov/alert/Rain/pcp_WY_YYYY.xlsx
   └─ Save to temp file

2. Parse Excel workbook
   ├─ Open with calamine::open_workbook_auto()
   ├─ Iterate 12 monthly sheets (OCT through SEP)
   ├─ Skip Annual_Totals sheet (summary only)
   └─ For each sheet:
      ├─ Read Row 3: Extract gauge IDs (columns B onward)
      ├─ Read Rows 4-34: Daily precipitation data
      └─ Stop at Row 35 (Totals row)

3. Extract data from each row
   ├─ Column A: Parse ISO date string "YYYY-MM-DD" → chrono::NaiveDate
   ├─ Columns B+: Parse rainfall values
   │  ├─ Numeric cells → f64 (e.g., 0.03937, 1.14173)
   │  ├─ Zero → 0.0 (valid reading, no rain)
   │  └─ Empty/null → Skip (shouldn't occur based on sample)
   └─ Handle high precision: Round to 5 decimal places (0.00001 inch precision)

4. Build reading records
   ├─ station_id: Gauge ID from Row 3 header
   ├─ reading_date: Parsed date from Column A
   ├─ rainfall_inches: Parsed value (or 0.0)
   ├─ data_source: "excel_WY_YYYY" (e.g., "excel_WY_2023")
   └─ import_metadata: JSONB with sheet name, row number

5. Parse footnotes (optional)
   ├─ Read rows 37+ until end of sheet
   ├─ Extract footnote text
   └─ Store in import_metadata if relevant to specific gauges

6. Validate data (same as PDF)

7. Bulk insert
   ├─ Batch 1000 rows at a time
   ├─ ON CONFLICT (station_id, reading_date) DO NOTHING
   ├─ Track: inserted, skipped (duplicates), errors
   └─ Commit transaction per sheet or per file
```

**Excel-Specific Parsing Notes**:
- **Date format**: ISO string `YYYY-MM-DD`, not Excel serial numbers
- **Reverse order**: Dates go from end of month → start (Oct 31 → Oct 1)
- **All gauges in one sheet**: ~350+ gauges vs. PDF's 8 per page
- **Column mapping**: Build map of column_index → station_id from Row 3
- **Performance**: Can process ~10,500 readings per sheet (350 gauges × 30 days)

**Calamine Implementation Example**:
```rust
use calamine::{Reader, open_workbook_auto, Xlsx, Data, Range};
use tokio::task;
use std::path::Path;

pub async fn import_water_year(path: &Path, water_year: u16) -> Result<Vec<Reading>> {
    let path = path.to_owned();

    // Spawn blocking task for Excel I/O
    let readings = task::spawn_blocking(move || {
        parse_excel_file(&path, water_year)
    }).await??;

    Ok(readings)
}

fn parse_excel_file(path: &Path, water_year: u16) -> Result<Vec<Reading>> {
    let mut workbook: Xlsx<_> = open_workbook_auto(path)?;
    let mut all_readings = Vec::new();

    // Process each monthly sheet
    for month in ["OCT", "NOV", "DEC", "JAN", "FEB", "MAR",
                  "APR", "MAY", "JUN", "JUL", "AUG", "SEP"] {
        if let Ok(range) = workbook.worksheet_range(month) {
            let readings = parse_monthly_sheet(range, month, water_year)?;
            all_readings.extend(readings);
        }
    }

    Ok(all_readings)
}

fn parse_monthly_sheet(range: Range<Data>, sheet_name: &str, wy: u16) -> Result<Vec<Reading>> {
    let mut readings = Vec::new();
    let rows: Vec<_> = range.rows().collect();

    // Row 3 (index 2): Extract gauge IDs from columns B onward
    let gauge_ids: Vec<String> = rows[2].iter()
        .skip(1)
        .filter_map(|cell| match cell {
            Data::Int(id) => Some(id.to_string()),
            Data::Float(id) => Some((*id as i64).to_string()),
            _ => None,
        })
        .collect();

    // Rows 4-34 (indices 3-33): Daily data
    for (row_idx, row) in rows.iter().enumerate().skip(3).take(31) {
        // Column A: Date string
        let date_str = match &row[0] {
            Data::String(s) => s,
            _ => continue,
        };
        let date = chrono::NaiveDate::parse_from_str(date_str, "%Y-%m-%d")?;

        // Columns B+: Rainfall values
        for (col_idx, cell) in row.iter().skip(1).enumerate() {
            if col_idx >= gauge_ids.len() { break; }

            let rainfall = match cell {
                Data::Float(val) => Some(*val),
                Data::Int(val) => Some(*val as f64),
                _ => None,
            };

            if let Some(rain) = rainfall {
                readings.push(Reading {
                    station_id: gauge_ids[col_idx].clone(),
                    reading_date: date,
                    rainfall_inches: (rain * 100000.0).round() / 100000.0, // Round to 5 decimals
                    data_source: format!("excel_WY_{}", wy),
                    import_metadata: Some(json!({
                        "sheet": sheet_name,
                        "row": row_idx + 1,
                    })),
                });
            }
        }
    }

    Ok(readings)
}
```

### 6. Data Validation Rules

```rust
// Validation criteria
struct ValidationRules {
    // Rainfall constraints
    min_rainfall: f64 = 0.00,
    max_rainfall: f64 = 20.00,  // Sanity check for extreme events

    // Date constraints
    min_date: NaiveDate,  // Start of historical records (e.g., 2000-01-01)
    max_date: NaiveDate,  // Today

    // Water year validation
    water_year_start_month: 10,  // October
    water_year_start_day: 1,
    water_year_end_month: 9,     // September
    water_year_end_day: 30,
}

// Validation errors
enum ValidationError {
    InvalidRainfall(f64),
    FutureDate(NaiveDate),
    InvalidStation(String),
    OutOfWaterYear { date: NaiveDate, water_year: u16 },
}
```

### 7. Error Handling & Transaction Strategy

**Strategy**: Best-effort import with detailed error tracking and per-batch transactions.

#### Transaction Boundaries

```
Parse Excel File (Blocking Task)
    ↓
Validate All Readings (collect errors, don't fail)
    ↓
Split into Batches of 1000
    ↓
For Each Batch:
    ├─ BEGIN TRANSACTION
    ├─ INSERT with ON CONFLICT DO NOTHING
    ├─ COMMIT (or ROLLBACK on error)
    └─ Continue to next batch (don't stop on error)
    ↓
Return ImportStats with full report
```

**Why per-batch transactions?**
- ✅ Resilient - one bad batch doesn't fail entire import
- ✅ Short transactions (~100ms per batch)
- ✅ Can continue after errors
- ✅ Partial success is useful (import what we can)
- ✅ Clear success/failure reporting

**Why not single transaction per file?**
- ❌ One bad row = entire file rejected
- ❌ Long-held transactions on large files
- ❌ Memory intensive
- ❌ Parsing error after 10k rows = wasted work

#### Import Statistics Tracking

```rust
#[derive(Debug, Default)]
pub struct ImportStats {
    // Success metrics
    pub files_processed: usize,
    pub rows_parsed: usize,
    pub rows_inserted: usize,
    pub rows_skipped_duplicate: usize,

    // Failure metrics
    pub rows_failed_validation: usize,
    pub rows_failed_insert: usize,

    // Detailed errors
    pub validation_errors: Vec<ValidationError>,
    pub insert_errors: Vec<InsertError>,
}

#[derive(Debug)]
pub struct ValidationError {
    pub file: String,
    pub row: usize,
    pub station_id: String,
    pub date: NaiveDate,
    pub reason: String,
}

#[derive(Debug)]
pub struct InsertError {
    pub batch_start_row: usize,
    pub batch_size: usize,
    pub reason: String,
}
```

#### Implementation Pattern

```rust
pub async fn import_water_year(
    &self,
    path: &Path,
    water_year: u16,
) -> Result<ImportStats> {
    let path = path.to_owned();

    // Parse file in blocking task
    let readings = tokio::task::spawn_blocking(move || {
        parse_excel_file(&path, water_year)
    })
    .await??;

    // Validate all readings (collect errors, don't fail)
    let (valid, invalid) = self.validate_readings(readings);

    let mut stats = ImportStats {
        rows_parsed: valid.len() + invalid.len(),
        rows_failed_validation: invalid.len(),
        validation_errors: invalid,
        ..Default::default()
    };

    // Insert in batches with individual error tracking
    for batch in valid.chunks(1000) {
        match self.insert_batch(batch).await {
            Ok((inserted, skipped)) => {
                stats.rows_inserted += inserted;
                stats.rows_skipped_duplicate += skipped;
            }
            Err(e) => {
                stats.rows_failed_insert += batch.len();
                stats.insert_errors.push(InsertError {
                    batch_start_row: batch[0].row_number,
                    batch_size: batch.len(),
                    reason: e.to_string(),
                });
                // Continue with next batch!
            }
        }
    }

    // Log summary
    info!(
        "Import complete: {} inserted, {} duplicates, {} validation errors, {} insert errors",
        stats.rows_inserted,
        stats.rows_skipped_duplicate,
        stats.rows_failed_validation,
        stats.rows_failed_insert
    );

    Ok(stats)
}

async fn insert_batch(&self, batch: &[Reading]) -> Result<(usize, usize)> {
    let mut tx = self.pool.begin().await?;

    let inserted = bulk_insert_batch(&mut tx, batch).await?;
    let skipped = batch.len() - inserted;

    tx.commit().await?;

    Ok((inserted, skipped))
}
```

### 8. Bulk Insert Optimization

**Batch Size**: 1000 rows per transaction

**Why 1000?**
- ✅ PostgreSQL handles well (not too large)
- ✅ Short transaction time (~100ms)
- ✅ Reasonable rollback cost if batch fails
- ✅ Good balance between throughput and safety

#### Efficient Bulk Insert with UNNEST

```rust
async fn bulk_insert_batch(
    tx: &mut Transaction<'_, Postgres>,
    batch: &[Reading],
) -> Result<usize> {
    // Use PostgreSQL UNNEST for efficient bulk insert
    let station_ids: Vec<_> = batch.iter().map(|r| &r.station_id).collect();
    let dates: Vec<_> = batch.iter().map(|r| r.reading_date).collect();
    let rainfalls: Vec<_> = batch.iter().map(|r| r.rainfall_inches).collect();
    let sources: Vec<_> = batch.iter().map(|r| &r.data_source).collect();
    let metadata: Vec<_> = batch.iter()
        .map(|r| r.import_metadata.as_ref())
        .collect();

    let result = sqlx::query!(
        r#"
        INSERT INTO rain_readings
            (station_id, reading_date, rainfall_inches, data_source, import_metadata)
        SELECT * FROM UNNEST($1::text[], $2::date[], $3::float8[], $4::text[], $5::jsonb[])
        ON CONFLICT (station_id, reading_date) DO NOTHING
        RETURNING id
        "#,
        &station_ids as &[&str],
        &dates as &[NaiveDate],
        &rainfalls as &[f64],
        &sources as &[&str],
        &metadata as &[Option<&serde_json::Value>],
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(result.len())  // Number of rows actually inserted (not duplicates)
}
```

**Benefits:**
- ✅ Single SQL statement for 1000 rows
- ✅ `ON CONFLICT DO NOTHING` provides idempotency
- ✅ Returns actual inserted count (excludes duplicates)
- ✅ Much faster than 1000 individual INSERTs
- ✅ PostgreSQL optimizes UNNEST efficiently

#### Alternative: PostgreSQL COPY (Optional Future Enhancement)

For even faster imports (10x speed improvement), consider PostgreSQL COPY:

```rust
// Use only if performance becomes an issue
async fn bulk_insert_with_copy(
    pool: &PgPool,
    readings: &[Reading],
) -> Result<()> {
    // Note: COPY doesn't support ON CONFLICT
    // Would need pre-filtering for duplicates
    // Recommended only for initial bulk import
}
```

**Trade-offs:**
- ✅ 10x faster than INSERT
- ✅ Best for millions of rows
- ❌ All-or-nothing (can't use ON CONFLICT)
- ❌ More complex error handling
- ❌ Overkill for our use case (~100k-200k rows)

**Recommendation:** Start with UNNEST (sufficient for our volume), consider COPY only if needed.

#### Progress Tracking

```rust
use indicatif::{ProgressBar, ProgressStyle};

let pb = ProgressBar::new(total_rows as u64);
pb.set_style(
    ProgressStyle::default_bar()
        .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} rows ({eta})")
        .unwrap()
        .progress_chars("##-")
);

for batch in readings.chunks(1000) {
    // Insert batch...
    pb.inc(batch.len() as u64);
}

pb.finish_with_message("Import complete");
```

## Technical Considerations

### PDF Parsing Challenges

1. **Multi-page layout**: Need to iterate through 40+ pages per month
2. **Table detection**: Fixed-width columns, varying gauge IDs per page
3. **Text extraction**: May require OCR if PDFs are scanned (unlikely)
4. **Missing data**: Underscores `____` or `_____(1)` with footnotes
5. **Special characters**: Footnote markers like `(1)`, `(2)`
6. **Varying formats**: Format may have changed over years

**Mitigation**:
- Use `pdf-extract` for text-based PDFs
- Regex patterns for gauge ID detection: `\d{4,5}`
- Parse footnotes separately and store in `import_metadata`
- Implement robust error handling for format variations

### Excel Parsing Considerations

**Confirmed Structure** (based on `pcp_WY_2023.xlsx` analysis):

1. **Date formats**: ✅ ISO date strings (`YYYY-MM-DD`), NOT Excel serial numbers
   - No date parsing issues - simple string parsing with chrono
   - Calamine returns dates as `Data::String` type

2. **Multiple sheets**: ✅ 12 monthly sheets (OCT-SEP) + 1 Annual_Totals
   - **Process**: Monthly sheets only (skip Annual_Totals)
   - **Order**: Reverse chronological in workbook, but consistent structure
   - Calamine: Use `workbook.worksheet_range(sheet_name)` to access by name

3. **Header detection**: ✅ Fixed structure
   - Row 1: Title
   - Row 2: Column numbers (1, 2, 3, ...)
   - Row 3: Gauge IDs (1000, 1200, 1500, ...)
   - Rows 4-34: Daily data
   - Calamine: Access via `range.rows()` iterator (zero-indexed)

4. **Empty cells**: ✅ Zeros for no rain
   - `0` = valid reading (no precipitation)
   - Based on sample, no truly empty cells in data range
   - Calamine: Returns `Data::Int(0)` or `Data::Float(0.0)` for zero values

5. **Formulas**: ✅ Totals row uses formulas
   - Calamine automatically reads calculated values (data_only mode is default)
   - Skip Row 35 (totals) since we calculate our own aggregations

6. **High precision values**: ⚠️ Many decimal places
   - Example: `2.6377899999` (likely floating-point representation)
   - Calamine returns as `Data::Float(f64)`
   - **Solution**: Round to 5 decimal places (0.00001" = ~0.25mm precision)
   - Formula: `(value * 100000.0).round() / 100000.0`

7. **Large column count**: ⚠️ 350+ gauge columns per sheet
   - **Solution**: Use column iterator, don't assume fixed width
   - Build dynamic map: `column_index → station_id`
   - Calamine efficiently handles wide ranges (A1:MZ49 ≈ 362 columns)

**Calamine-Specific Implementation Details**:

```rust
// Opening workbook
let mut workbook: Xlsx<_> = open_workbook_auto(path)?;

// Access sheet by name
let range: Range<Data> = workbook.worksheet_range("OCT")?;

// Iterate rows (0-indexed)
for row in range.rows() {
    for cell in row {
        match cell {
            Data::Int(i) => { /* Integer value */ },
            Data::Float(f) => { /* Float value */ },
            Data::String(s) => { /* String value */ },
            Data::Bool(b) => { /* Boolean value */ },
            Data::DateTime(dt) => { /* Excel datetime */ },
            Data::Duration(d) => { /* Excel duration */ },
            Data::DateTimeIso(s) => { /* ISO datetime string */ },
            Data::DurationIso(s) => { /* ISO duration string */ },
            Data::Error(e) => { /* Cell error */ },
            Data::Empty => { /* Empty cell */ },
        }
    }
}
```

**Key Advantages of Calamine**:
- ✅ **Pure Rust**: No external dependencies (libxlsreader, etc.)
- ✅ **Type-safe**: `Data` enum for all cell types
- ✅ **Performance**: Lazy loading, efficient memory usage
- ✅ **Chrono integration**: `dates` feature for datetime handling
- ✅ **Multiple formats**: xlsx, xlsb, xls, ods in one API
- ✅ **Tokio compatible**: Works with `spawn_blocking()`

**Integration Pattern**:
```rust
// In async context
pub async fn import_excel(path: &Path) -> Result<Vec<Reading>> {
    let path = path.to_owned();

    // Spawn blocking task for file I/O
    let readings = tokio::task::spawn_blocking(move || {
        // Synchronous calamine code here
        parse_excel_sync(&path)
    }).await??;

    // Back in async context - use SQLx for DB operations
    bulk_insert_readings(&readings).await?;

    Ok(readings)
}
```

### Data Quality Issues

1. **Gauge outages**: Missing data periods
2. **Estimated values**: Noted in footnotes
3. **Duplicate data**: Overlap with live scraping
4. **Data corrections**: MCFCD may update historical files
5. **Timezone**: All dates in Arizona MST (no DST)

**Mitigation**:
- Store footnotes in `import_metadata` JSONB
- Use UPSERT logic (ON CONFLICT DO NOTHING or DO UPDATE)
- Allow re-import of years to pick up corrections
- Document timezone assumptions

## Import Strategy

### Initial Bulk Import (Years 2010-2024)

**Recommended Approach:**

```bash
# Option 1: Using helper script (simplest)
./scripts/import-bulk.sh

# Option 2: Direct kubectl command
kubectl create -f k8s/jobs/historical-bulk-import.yaml

# Monitor progress
kubectl logs -f -l job-type=historical-bulk-import

# Check status
kubectl get jobs -l job-type=historical-bulk-import
```

**This will:**
- Import all PDF files (Oct 2010 - Sep 2021)
- Import all Excel files (Oct 2021 - Sep 2024)
- Handle ~14 years × 12 months = ~168 months of data
- Take approximately 30-60 minutes depending on network and processing speed

### Incremental Updates

**Re-import Specific Water Year** (if MCFCD updates/corrects data):

```bash
# Option 1: Using helper script
./scripts/import-water-year.sh 2023

# Option 2: Direct kubectl with inline edit
cat k8s/jobs/historical-single-year-import.yaml | \
  sed 's/value: "2023"/value: "2023"/' | \
  kubectl create -f -

# Monitor
kubectl logs -f -l job-type=historical-single-year
```

**Import New Water Year Annually** (after October 1):

```bash
# When new water year data becomes available
./scripts/import-water-year.sh 2025
```

### Verification Queries

```sql
-- Check import coverage
SELECT
    data_source,
    DATE_TRUNC('month', reading_date) AS month,
    COUNT(*) AS reading_count,
    COUNT(DISTINCT station_id) AS gauge_count
FROM rain_readings
WHERE data_source != 'live_scrape'
GROUP BY data_source, month
ORDER BY month DESC;

-- Find gaps in historical data
SELECT
    station_id,
    reading_date,
    LAG(reading_date) OVER (PARTITION BY station_id ORDER BY reading_date) AS prev_date,
    reading_date - LAG(reading_date) OVER (PARTITION BY station_id ORDER BY reading_date) AS gap_days
FROM rain_readings
WHERE data_source != 'live_scrape'
  AND gap_days > 1
ORDER BY gap_days DESC;

-- Compare live vs historical data for overlap period
SELECT
    station_id,
    reading_date,
    data_source,
    rainfall_inches
FROM rain_readings
WHERE reading_date BETWEEN '2024-01-01' AND '2024-12-31'
  AND station_id = '59700'
ORDER BY reading_date;
```

## Benefits of This Design

1. **Gap Filling**: Automatically fills missing days from live scraping failures
2. **Historical Analysis**: Enables long-term trend analysis (10+ years)
3. **Water Year Comparisons**: Compare current year to historical averages
4. **Monthly Summaries**: Existing `monthly_rainfall_summary` auto-populates
5. **Idempotent**: Safe to run multiple times (deduplication built-in)
6. **Flexibility**: Can re-import years if MCFCD updates data
7. **Data Provenance**: `data_source` column tracks origin of each reading
8. **Metadata Preservation**: Footnotes and gauge outages preserved in JSONB

## Error Handling Strategy

```rust
// Import result tracking
struct ImportStats {
    total_rows_processed: usize,
    rows_inserted: usize,
    rows_skipped_duplicate: usize,
    rows_failed_validation: usize,
    errors: Vec<ImportError>,
}

enum ImportError {
    DownloadFailed { url: String, error: String },
    ParseFailed { file: String, page: usize, error: String },
    ValidationFailed { row: usize, error: ValidationError },
    DatabaseError { error: String },
}

// Logging strategy
// - INFO: Import started, file processed, stats summary
// - WARN: Validation failures, skipped rows
// - ERROR: Fatal errors (download failed, DB connection lost)
// - DEBUG: Individual row processing (--verbose flag)
```

## Testing Strategy

1. **Unit Tests**: PDF/Excel parsers with sample files
2. **Integration Tests**: End-to-end import with test database
3. **Validation Tests**: Edge cases (missing data, footnotes, estimates)
4. **Performance Tests**: Bulk import speed (target: 10k rows/sec)
5. **Idempotency Tests**: Re-import same file produces same result

## Deployment Checklist

### Database Setup
- [ ] Run database migration to add `data_source` and `import_metadata` columns
- [ ] Verify migration applied successfully

### Local Development & Testing
- [ ] Build and test `historical_import` binary locally
- [ ] Test PDF parsing with `plans/pcp1119.pdf`
- [ ] Test Excel parsing with `plans/pcp_WY_2023.xlsx`
- [ ] Verify downloads from MCFCD URLs work
- [ ] Test dry-run mode: `./historical_import --mode bulk --dry-run`

### Docker & K8s Preparation
- [ ] Build Docker image with new binary
- [ ] Push image to GitHub Container Registry
- [ ] Verify image includes `historical_import` binary at `/app/historical_import`
- [ ] Create K8s job manifests in `k8s/jobs/`
- [ ] Create helper scripts in `scripts/` and make executable

### Initial Test Import
- [ ] Deploy test job for single water year (WY 2023)
  ```bash
  ./scripts/import-water-year.sh 2023
  ```
- [ ] Monitor logs: `kubectl logs -f -l job-type=historical-single-year`
- [ ] Verify data in database:
  ```sql
  SELECT COUNT(*), MIN(reading_date), MAX(reading_date)
  FROM rain_readings
  WHERE data_source LIKE 'excel_WY_2023';
  ```
- [ ] Check for errors in import_metadata JSONB column

### Bulk Import
- [ ] Run bulk import job for all years (2010-2024)
  ```bash
  ./scripts/import-bulk.sh
  ```
- [ ] Monitor progress (may take 30-60 minutes)
- [ ] Verify import completed successfully
- [ ] Check total records imported
- [ ] Validate data coverage across all years

### Post-Import Verification
- [ ] Run verification queries (see "Verification Queries" section)
- [ ] Check for data gaps
- [ ] Update `monthly_rainfall_summary` view/table if needed
- [ ] Compare live vs historical data for overlap periods
- [ ] Verify `data_source` values are correct

### Documentation & Cleanup
- [ ] Document import process in CLAUDE.md or README.md
- [ ] Add troubleshooting notes for common issues
- [ ] Clean up old completed jobs:
  ```bash
  kubectl delete jobs -l app=rain-tracker --field-selector status.successful=1
  ```
- [ ] Document when to re-run imports (annual water year updates)

## Future Enhancements

1. **Automated Annual Import**: CronJob to import new water year data in October
   ```yaml
   # k8s/cronjobs/annual-historical-import.yaml
   apiVersion: batch/v1
   kind: CronJob
   metadata:
     name: annual-historical-import
   spec:
     schedule: "0 2 1 10 *"  # 2 AM on Oct 1 every year
     jobTemplate:
       spec:
         template:
           spec:
             containers:
             - name: importer
               image: ghcr.io/your-org/rain-tracker-service:latest
               command: ["/app/historical_import"]
               args:
                 - "--mode=single"
                 - "--water-year=$(date +%Y)"
   ```

2. **Data Reconciliation**: Compare live vs historical for accuracy
3. **API Endpoint**: `/api/v1/historical/import` to trigger imports via REST API
4. **Import Dashboard**: Web UI to monitor import status and view statistics
5. **Data Quality Reports**: Identify and flag suspicious readings (outliers, anomalies)
6. **Machine Learning**: Anomaly detection for gauge malfunctions
7. **Export Capability**: Generate reports from historical data (CSV, Excel)

## Technology Decisions

### Excel Parser: Calamine 0.31

**Decision**: Use `calamine` crate for Excel parsing

**Rationale**:
1. **Active Maintenance**: Last updated September 2025, actively maintained
2. **Architecture Fit**: Works seamlessly with our Tokio/Axum/SQLx stack via `spawn_blocking()`
3. **Performance**: Fast (<1s for 1MB files) and memory-efficient (~100MB)
4. **Pure Rust**: No C dependencies, safe and portable across platforms
5. **Read-Only**: Perfect for our import-only use case (vs. umya-spreadsheet which adds write complexity)
6. **Chrono Integration**: Built-in date parsing support with our existing chrono dependency
7. **Multiple Formats**: Supports xlsx, xlsb, xls, ods in one API

**Alternatives Considered**:
- `umya-spreadsheet`: Slower (6s+ vs <1s), memory-heavy (1GB+ vs 100MB), read+write (overkill)
- Custom parser: Too much effort, reinventing the wheel

**Integration Pattern**:
```rust
// Async context → spawn_blocking → sync parsing → back to async for DB
tokio::task::spawn_blocking(move || parse_excel())
    .await?
    .then(|readings| sqlx_bulk_insert(readings))
```

**Verification**:
- ✅ Tested with `plans/pcp_WY_2023.xlsx`
- ✅ Successfully parses 13 sheets (12 monthly + Annual_Totals)
- ✅ Handles 350+ gauge columns efficiently
- ✅ Parses ISO date strings correctly
- ✅ Returns typed data via `Data` enum

## References

- Maricopa County ALERT System: https://alert.fcd.maricopa.gov
- PDF Reports: https://alert.fcd.maricopa.gov/alert/Rain/pcpMMYY.pdf
- Excel Reports: https://alert.fcd.maricopa.gov/alert/Rain/pcp_WY_YYYY.xlsx
- Water Year Definition: October 1 (year-1) through September 30 (year)
- Sample Files: `plans/pcp1119.pdf`, `plans/pcp_WY_2023.xlsx`
- Calamine Crate: https://github.com/tafia/calamine (v0.31.0, MIT license)

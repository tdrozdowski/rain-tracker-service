# FOPR (Full Operational Period of Record) Import Plan

## Overview

This plan describes the implementation of a bulk import system for gauge-specific FOPR (Full Operational Period of Record) Excel files from the Maricopa County Flood Control District (MCFCD). Unlike the water year files which contain all gauges for a single year, FOPR files contain all historical data for a single gauge across all years of operation.

**Key Difference from Water Year Import:**
- **Water Year Files**: All gauges × 1 year (wide and shallow)
- **FOPR Files**: 1 gauge × all years (narrow and deep)

## Data Format Analysis

### FOPR File Format

**File Naming**: `{gauge_id}_FOPR.xlsx` where:
- `{gauge_id}` = 5-digit station ID (e.g., 59700, 11000, 89500)
- `FOPR` = Full Operational Period of Record
- Example: `59700_FOPR.xlsx`, `11000_FOPR.xlsx`

**URL Pattern**:
```
https://alert.fcd.maricopa.gov/alert/Rain/FOPR/{gauge_id}_FOPR.xlsx
```

**Examples**:
- `https://alert.fcd.maricopa.gov/alert/Rain/FOPR/59700_FOPR.xlsx`
- `https://alert.fcd.maricopa.gov/alert/Rain/FOPR/11000_FOPR.xlsx`
- `https://alert.fcd.maricopa.gov/alert/Rain/FOPR/89500_FOPR.xlsx`

### Workbook Structure

**Sample File**: `sample-data-files/59700_FOPR.xlsx`

**Workbook Layout** (TBD - needs analysis):
```
Sheet 1: Summary or metadata (TBD)
Sheet 2: Year 2024 (or most recent year)
Sheet 3: Year 2023
Sheet 4: Year 2022
...
Sheet N: First year of operation
```

**Expected Structure** (to be confirmed by analysis):
- **Multiple sheets**: One sheet per year of gauge operation
- **Sheet naming**: Likely year numbers (e.g., "2024", "2023", "2022")
- **Data format**: Daily rainfall readings, similar to water year Excel format
- **Date range**: January 1 - December 31 (calendar year, not water year)
- **Single gauge**: All data is for one station_id

**Structure Analysis Needed**:
- [ ] Examine `59700_FOPR.xlsx` to confirm sheet structure
- [ ] Determine date format (ISO strings vs. Excel serial numbers)
- [ ] Identify header row structure (dates in rows vs columns)
- [ ] Check for footnotes/metadata sheets
- [ ] Verify data range (daily vs monthly aggregates)
- [ ] Confirm year coverage (earliest to latest year available)

## Gauge Discovery Strategy

### Problem: Finding All FOPR Files

Unlike water year files which have a known pattern (years 2010-2024), FOPR files exist per gauge, and we need to discover which gauges have FOPR files available.

### Discovery Approaches

#### Approach 1: Use Existing Gauge Registry (Recommended)

Query the database for all known gauges and attempt to download FOPR for each:

```sql
-- Get all unique gauge IDs from database
SELECT DISTINCT station_id
FROM rain_readings
ORDER BY station_id;
```

**Pros**:
- ✅ Uses gauges we already know exist
- ✅ No need for web scraping
- ✅ Gauges are already validated

**Cons**:
- ⚠️ May miss gauges that only exist in FOPR but not in recent data
- ⚠️ Relies on having some historical data first

#### Approach 2: Systematic Enumeration

Try common gauge ID patterns and handle 404 gracefully:

```
Patterns to try:
- 5-digit: 10000-99999 (but this is 90,000 requests - too many)
- Known ranges: 59000-59999, 11000-11999, etc.
- Common prefixes: 59xxx, 11xxx, 1xxxx, etc.
```

**Pros**:
- ✅ Discovers all gauges including obscure ones

**Cons**:
- ❌ Too many HTTP requests (potentially 90,000+)
- ❌ MCFCD may rate-limit or block
- ❌ Inefficient and slow

#### Approach 3: Hybrid Approach (Best)

1. **Start with known gauges** from database
2. **Download available FOPR files**
3. **Log 404s** to identify gaps
4. **Manually add** any missing gauge IDs discovered from other sources

```rust
// Pseudo-code
let known_gauges = fetch_all_station_ids_from_db().await?;
let mut successful_downloads = Vec::new();
let mut not_found = Vec::new();

for gauge_id in known_gauges {
    match download_fopr(&gauge_id).await {
        Ok(bytes) => successful_downloads.push(gauge_id),
        Err(e) if e.is_404() => not_found.push(gauge_id),
        Err(e) => return Err(e),  // Real error, stop
    }
}

info!("Downloaded {} FOPR files", successful_downloads.len());
info!("Not found: {} gauges", not_found.len());
```

**Recommended**: Approach 3 (Hybrid)

## CLI Design

### Command Structure

```bash
# Import FOPR for single gauge
./fopr-import --mode single --gauge-id 59700

# Import FOPR for all known gauges
./fopr-import --mode all

# Import FOPR for specific list of gauges
./fopr-import --mode list --gauge-ids 59700,11000,89500

# Import from local file (testing)
./fopr-import --mode file --gauge-id 59700 --file sample-data-files/59700_FOPR.xlsx

# Save downloaded files
./fopr-import --mode all --save-files --output-dir ~/fopr-data

# Dry run (validate without inserting)
./fopr-import --mode all --dry-run

# Skip confirmation prompt
./fopr-import --mode all -y

# Verbose logging
./fopr-import --mode all --verbose
```

### CLI Arguments

```rust
#[derive(Parser)]
#[command(name = "fopr-import")]
#[command(about = "Import Full Operational Period of Record data from MCFCD", long_about = None)]
struct Cli {
    /// Database connection string
    #[arg(long, env)]
    database_url: String,

    /// Import mode: 'single' (one gauge), 'all' (all known gauges), 'list' (specific gauges), 'file' (local file)
    #[arg(long)]
    mode: String,

    /// Gauge ID for single mode
    #[arg(long)]
    gauge_id: Option<String>,

    /// Comma-separated gauge IDs for list mode
    #[arg(long)]
    gauge_ids: Option<String>,

    /// Path to local FOPR file (for 'file' mode)
    #[arg(long)]
    file: Option<PathBuf>,

    /// Save downloaded files instead of deleting them
    #[arg(long)]
    save_files: bool,

    /// Directory to save downloaded files (default: /tmp/fopr)
    #[arg(long, default_value = "/tmp/fopr")]
    output_dir: String,

    /// Skip confirmation prompt
    #[arg(short = 'y', long)]
    yes: bool,

    /// Dry run (validate without inserting)
    #[arg(long)]
    dry_run: bool,

    /// Verbose logging
    #[arg(short = 'v', long)]
    verbose: bool,

    /// Maximum concurrent downloads (default: 5)
    #[arg(long, default_value = "5")]
    max_concurrent: usize,
}
```

## Implementation Components

### Project Structure

```
src/
├── bin/
│   └── fopr_import.rs              // Main CLI entry point
├── importers/
│   ├── fopr_importer.rs            // FOPR Excel parsing logic
│   ├── fopr_downloader.rs          // HTTP download for FOPR files
│   └── gauge_registry.rs           // Query known gauges from DB
└── db/
    └── historical_repository.rs    // Reuse existing bulk insert

scripts/
└── import-fopr.sh                  // Helper: run FOPR import

sample-data-files/
└── 59700_FOPR.xlsx                 // Sample file for testing
```

### Rust Dependencies

Already have from `historical-import`:
```toml
[dependencies]
calamine = { version = "0.31", features = ["dates"] }
clap = { version = "4.5", features = ["derive"] }
indicatif = "0.17"
reqwest = { version = "0.12", features = ["json"] }
tokio = { version = "1", features = ["full"] }
sqlx = { version = "0.8", features = ["runtime-tokio-rustls", "postgres"] }
```

New dependency for concurrent downloads:
```toml
futures = "0.3"
tokio-stream = "0.1"
```

## Data Processing Pipeline

### FOPR Import Flow

```
1. Determine gauge list
   ├─ Mode: single → Use provided gauge_id
   ├─ Mode: all → Query database for all station_ids
   ├─ Mode: list → Parse comma-separated gauge_ids
   └─ Mode: file → Use gauge_id from --gauge-id arg

2. Download FOPR files (parallel, max 5 concurrent)
   ├─ URL: https://alert.fcd.maricopa.gov/alert/Rain/FOPR/{gauge_id}_FOPR.xlsx
   ├─ Handle 404s gracefully (gauge may not have FOPR)
   ├─ Retry on transient errors (3 attempts)
   ├─ Progress bar showing: X/Y gauges downloaded
   └─ Save to output_dir if --save-files specified

3. Parse FOPR Excel file (blocking task)
   ├─ Open workbook with calamine
   ├─ Identify all year sheets (skip summary/metadata sheets)
   ├─ For each year sheet:
   │  ├─ Parse header row (identify date column and data columns)
   │  ├─ Extract daily readings
   │  ├─ Build Reading records with station_id from filename
   │  └─ Track year and month of each reading
   └─ Aggregate all readings across all years

4. Validate data
   ├─ Date within reasonable range (2000-present)
   ├─ Rainfall values 0.00-20.00 inches
   ├─ No future dates
   └─ Station ID matches filename

5. Build summary statistics
   ├─ Group by year and month
   ├─ Count readings with rainfall > 0
   ├─ Track coverage: which years and months have data
   └─ Store for final report

6. Bulk insert (if not --dry-run)
   ├─ Batch 1000 rows at a time
   ├─ ON CONFLICT (station_id, reading_date) DO NOTHING
   ├─ Track: inserted, skipped (duplicates), errors
   └─ Commit transaction per batch

7. Print summary report
   ├─ Table showing gauge coverage by year/month
   ├─ Total readings per gauge
   └─ Years and months with readings > 0
```

### Concurrent Download Strategy

```rust
use futures::stream::{self, StreamExt};
use tokio::sync::Semaphore;

async fn download_all_fopr(
    gauge_ids: Vec<String>,
    max_concurrent: usize,
) -> Result<Vec<(String, Vec<u8>)>> {
    let semaphore = Arc::new(Semaphore::new(max_concurrent));
    let client = reqwest::Client::new();

    let tasks = gauge_ids.into_iter().map(|gauge_id| {
        let semaphore = semaphore.clone();
        let client = client.clone();

        async move {
            let _permit = semaphore.acquire().await?;
            download_fopr_for_gauge(&client, &gauge_id).await
        }
    });

    // Execute with progress bar
    let results = stream::iter(tasks)
        .buffer_unordered(max_concurrent)
        .collect::<Vec<_>>()
        .await;

    Ok(results.into_iter().filter_map(Result::ok).collect())
}
```

### FOPR Parsing Logic

**TBD after analyzing `59700_FOPR.xlsx`** - structure to be determined:

```rust
pub struct FoprImporter {
    file_path: String,
}

impl FoprImporter {
    pub fn new(file_path: &str) -> Self {
        Self {
            file_path: file_path.to_string(),
        }
    }

    /// Parse all years from FOPR file
    pub fn parse_all_years(
        &self,
        gauge_id: &str,
    ) -> Result<Vec<HistoricalReading>, Box<dyn std::error::Error>> {
        let mut workbook: Xlsx<_> = open_workbook_auto(&self.file_path)?;
        let mut all_readings = Vec::new();

        // TBD: Determine year sheet naming pattern
        // Option 1: Sheets named by year ("2024", "2023", etc.)
        // Option 2: Sheets in sequence (Sheet1, Sheet2, etc.)
        // Option 3: Mixed (summary sheet + year sheets)

        for sheet_name in workbook.sheet_names().to_owned() {
            // Skip summary/metadata sheets
            if is_summary_sheet(&sheet_name) {
                continue;
            }

            let range = workbook.worksheet_range(&sheet_name)?;
            let year = parse_year_from_sheet(&sheet_name)?;

            let readings = self.parse_year_sheet(range, gauge_id, year)?;
            all_readings.extend(readings);
        }

        Ok(all_readings)
    }

    fn parse_year_sheet(
        &self,
        range: Range<Data>,
        gauge_id: &str,
        year: i32,
    ) -> Result<Vec<HistoricalReading>> {
        // TBD: Implement based on actual FOPR structure
        // Expected: Similar to water year Excel but for single gauge
        unimplemented!("Needs analysis of 59700_FOPR.xlsx structure")
    }
}
```

## Summary Report Format

### Gauge Coverage Table

The import should produce a summary showing which years and months have readings > 0 for each gauge:

```
================================================================================
FOPR Import Summary
================================================================================
Gauge ID: 59700
Total readings: 12,456
Years covered: 2010-2024 (15 years)
================================================================================

Year  Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec  Total
--------------------------------------------------------------------------------
2024   31  29  31  30  31  30   5   0   0   0   0   0    187
2023   31  28  31  30  31  30  31  31  30  31  30  31    365
2022   31  28  31  30  31  30  31  31  30  31  30  31    365
2021   31  28  31  30  31  30  31  31  30  31  30  31    365
2020   31  29  31  30  31  30  31  31  30  31  30  31    366
...
2010    0   0   0  15  30  31  31  31  30  31  30  31    260
--------------------------------------------------------------------------------
Total 465 421 465 450 465 450 465 465 450 465 450 465  5,480

Months with rainfall > 0:
  2024: Jan, Feb, Mar, Apr, May, Jun, Jul (7 months)
  2023: Jan, Feb, Mar, Jul, Aug, Sep, Oct, Dec (8 months)
  2022: Jan, Feb, Jul, Aug, Sep, Dec (6 months)
  ...

================================================================================
```

### Multi-Gauge Summary (for --mode all)

```
================================================================================
FOPR Bulk Import Summary
================================================================================
Gauges processed:    45
Gauges successful:   42
Gauges not found:     3 (no FOPR file available)

Total readings imported: 524,187
Duplicates skipped:       12,456
Validation errors:           123
--------------------------------------------------------------------------------

Per-Gauge Summary:
Gauge ID  Years     Readings  Months w/ Rain  Status
--------  --------  --------  --------------  ------
59700     2010-2024  12,456           98/180  ✓
11000     2012-2024   9,234           82/156  ✓
89500     2015-2024   6,789           54/120  ✓
...
1000      -              0            0/0     ✗ Not Found
1200      -              0            0/0     ✗ Not Found
1300      2020-2024   1,234           12/60   ✓

================================================================================
Downloaded FOPR files saved to: ~/fopr-data/
================================================================================
```

## Implementation Phases

### Phase 1: Structure Analysis (Day 1)

**Goal**: Understand FOPR file format

Tasks:
- [ ] Examine `sample-data-files/59700_FOPR.xlsx` manually
- [ ] Document sheet structure, naming, and layout
- [ ] Identify header rows, data ranges, and date formats
- [ ] Check for footnotes and metadata sheets
- [ ] Write structure analysis notes in this plan document
- [ ] Create parsing strategy based on findings

### Phase 2: Core Parsing (Day 2)

**Goal**: Parse single FOPR file from local disk

Tasks:
- [ ] Implement `FoprImporter` struct and parsing logic
- [ ] Handle multiple year sheets
- [ ] Extract daily readings with proper date parsing
- [ ] Validate data structure and ranges
- [ ] Unit tests with `59700_FOPR.xlsx`
- [ ] Verify readings count and date ranges

### Phase 3: Download & Discovery (Day 3)

**Goal**: Download FOPR files from MCFCD

Tasks:
- [ ] Implement `FoprDownloader` with 404 handling
- [ ] Query database for known gauge IDs
- [ ] Concurrent download with semaphore (max 5)
- [ ] Progress bar for download status
- [ ] Handle errors gracefully (404 = skip, 500 = retry)
- [ ] Test with 5-10 gauges first

### Phase 4: CLI & Integration (Day 4)

**Goal**: Complete CLI with all modes

Tasks:
- [ ] Implement CLI argument parsing
- [ ] Mode: single (one gauge)
- [ ] Mode: all (all known gauges)
- [ ] Mode: list (specific gauge list)
- [ ] Mode: file (local file testing)
- [ ] --save-files and --output-dir support
- [ ] --dry-run mode
- [ ] Confirmation prompts

### Phase 5: Summary Reporting (Day 5)

**Goal**: Generate coverage summary tables

Tasks:
- [ ] Build year/month coverage matrix
- [ ] Count readings per month
- [ ] Identify months with rainfall > 0
- [ ] Format table output (see "Summary Report Format")
- [ ] Multi-gauge summary for bulk imports
- [ ] Export summary to CSV/JSON (optional)

### Phase 6: Testing & Polish (Day 6)

**Goal**: Production-ready binary

Tasks:
- [ ] Integration tests with test database
- [ ] Test all CLI modes
- [ ] Test concurrent downloads (10+ gauges)
- [ ] Verify deduplication works correctly
- [ ] Performance testing (should handle 50+ gauges)
- [ ] Error handling and recovery
- [ ] Documentation in CLAUDE.md

## Usage Examples

### Single Gauge Import

```bash
# Import FOPR for gauge 59700
DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker" \
./target/debug/fopr-import \
  --mode single \
  --gauge-id 59700 \
  -y

# Expected output:
Downloading FOPR for gauge 59700...
✓ Downloaded 59700_FOPR.xlsx (2.3 MB)
Parsing FOPR file...
✓ Parsed 12,456 readings across 15 years (2010-2024)
Inserting into database...
✓ Inserted 11,234 new readings, 1,222 duplicates skipped

Gauge Coverage Summary - 59700
Year  Jan Feb Mar Apr May Jun Jul Aug Sep Oct Nov Dec  Total
2024   31  29  31  30  31  30   5   0   0   0   0   0    187
2023   31  28  31  30  31  30  31  31  30  31  30  31    365
...

Months with rainfall > 0: 98/180 (54%)
```

### Bulk Import All Gauges

```bash
# Import all known gauges
DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker" \
./target/debug/fopr-import \
  --mode all \
  --save-files \
  --output-dir ~/fopr-data \
  -y

# Expected output:
Fetching known gauges from database...
Found 45 gauges
Downloading FOPR files (max 5 concurrent)...
[00:02:34] ########################################## 45/45
✓ Downloaded 42 files (3 not found)

Parsing FOPR files...
[00:01:12] ########################################## 42/42
✓ Parsed 524,187 total readings

Inserting into database...
[00:03:45] ######################################## 524187/524187
✓ Inserted 511,731 new readings, 12,456 duplicates skipped

FOPR Bulk Import Summary
Gauges processed:    45
Gauges successful:   42
Total readings:      524,187
...
```

### Test with Local File

```bash
# Test parsing without downloading
./target/debug/fopr-import \
  --mode file \
  --gauge-id 59700 \
  --file sample-data-files/59700_FOPR.xlsx \
  --dry-run

# Expected output:
Parsing local FOPR file: sample-data-files/59700_FOPR.xlsx
Gauge ID: 59700
✓ Parsed 12,456 readings across 15 years

DRY RUN - No data inserted to database

Coverage Summary:
Years: 2010-2024
Months with data: 180
Months with rain: 98 (54%)
```

## Database Integration

### Reuse Existing Schema

No new tables needed - reuse `rain_readings`:

```sql
-- Data source for FOPR imports
-- Format: 'fopr_{gauge_id}' (e.g., 'fopr_59700')
INSERT INTO rain_readings (
    station_id,
    reading_datetime,
    cumulative_inches,
    incremental_inches,
    data_source,
    import_metadata
)
VALUES (
    '59700',
    '2023-01-15',
    0.0,  -- FOPR files have daily incremental, not cumulative
    1.23,
    'fopr_59700',
    '{"sheet": "2023", "row": 15, "source_file": "59700_FOPR.xlsx"}'
);
```

### Deduplication Strategy

Use existing unique constraint:
```sql
-- Existing constraint handles deduplication
UNIQUE (reading_datetime, station_id)
```

If a reading already exists (from water year import or live scraping), the FOPR import will skip it (ON CONFLICT DO NOTHING).

**Question**: Should FOPR data override existing data?
- **No (recommended)**: Use `ON CONFLICT DO NOTHING` - prefer most recent import
- **Yes**: Use `ON CONFLICT DO UPDATE` - FOPR is authoritative

**Decision**: Use `ON CONFLICT DO NOTHING` for safety. User can manually delete conflicting data if they want FOPR to override.

## Verification Queries

```sql
-- Check FOPR import coverage
SELECT
    station_id,
    COUNT(*) AS total_readings,
    MIN(reading_date) AS earliest_date,
    MAX(reading_date) AS latest_date,
    COUNT(DISTINCT EXTRACT(YEAR FROM reading_date)) AS year_count
FROM rain_readings
WHERE data_source LIKE 'fopr_%'
GROUP BY station_id
ORDER BY station_id;

-- Find months with rainfall > 0 for specific gauge
SELECT
    EXTRACT(YEAR FROM reading_date) AS year,
    EXTRACT(MONTH FROM reading_date) AS month,
    COUNT(*) AS readings,
    COUNT(*) FILTER (WHERE incremental_inches > 0) AS rainy_days,
    SUM(incremental_inches) AS total_rainfall
FROM rain_readings
WHERE station_id = '59700'
  AND data_source = 'fopr_59700'
GROUP BY year, month
ORDER BY year, month;

-- Compare FOPR vs water year data for overlaps
SELECT
    station_id,
    reading_date,
    data_source,
    incremental_inches
FROM rain_readings
WHERE station_id = '59700'
  AND reading_date BETWEEN '2023-01-01' AND '2023-12-31'
  AND data_source IN ('fopr_59700', 'excel_WY_2023', 'pdf_WY_2023')
ORDER BY reading_date;
```

## Benefits

1. **Complete Historical Record**: Full gauge history from first operation to present
2. **Gauge-Specific Analysis**: Enables per-gauge trend analysis across decades
3. **Gap Filling**: Fills gaps not covered by water year or PDF imports
4. **Data Validation**: Compare FOPR vs water year data for accuracy
5. **Operational Insights**: See when gauges were installed, decommissioned, or had outages
6. **Rainfall Patterns**: Identify seasonal patterns and long-term climate trends

## Future Enhancements

1. **Data Reconciliation**: Compare FOPR vs water year imports, flag discrepancies
2. **Coverage Visualization**: Generate charts showing gauge coverage over time
3. **Incremental Updates**: Re-download FOPR annually to get latest year
4. **API Endpoint**: `/api/v1/fopr/import/{gauge_id}` to trigger imports via REST
5. **Gauge Discovery**: Auto-detect new gauges by monitoring MCFCD gauge list page
6. **Export Reports**: Generate CSV/Excel reports of gauge coverage
7. **Data Quality Checks**: Identify suspicious gaps or anomalies in FOPR data

## Open Questions

1. **FOPR File Structure**: Need to analyze `59700_FOPR.xlsx` to confirm:
   - Sheet naming convention
   - Date format (ISO strings vs Excel serial)
   - Data layout (dates in rows vs columns)
   - Presence of summary/metadata sheets

2. **Gauge Discovery**: How to find all gauges with FOPR files?
   - Use database query (recommended)
   - Or scrape MCFCD gauge list page
   - Or enumerate common ID ranges

3. **Cumulative vs Incremental**: Do FOPR files contain:
   - Daily incremental rainfall (most likely)
   - Cumulative monthly totals
   - Both

4. **Date Range**: What is typical FOPR coverage?
   - From gauge installation to present
   - Fixed historical period (e.g., 2000-present)
   - Variable by gauge

5. **Update Frequency**: How often does MCFCD update FOPR files?
   - Annually (after water year ends)
   - Monthly (rolling updates)
   - On-demand (manual updates)

## Next Steps

1. **Analyze Sample File**: Examine `59700_FOPR.xlsx` to answer open questions
2. **Update This Plan**: Document findings and finalize structure
3. **Implement Parser**: Build `FoprImporter` based on confirmed structure
4. **Test Locally**: Verify parsing with sample file
5. **Implement Downloader**: Add HTTP download with 404 handling
6. **Build CLI**: Complete all modes and options
7. **Test End-to-End**: Run full import for 5-10 gauges
8. **Production Deploy**: Run bulk import for all gauges

## References

- Sample File: `sample-data-files/59700_FOPR.xlsx`
- URL Pattern: `https://alert.fcd.maricopa.gov/alert/Rain/FOPR/{gauge_id}_FOPR.xlsx`
- Related Plan: `plans/historical-data-import.md` (water year import)
- Calamine Crate: https://github.com/tafia/calamine

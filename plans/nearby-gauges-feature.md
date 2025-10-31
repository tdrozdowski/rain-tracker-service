# Nearby Gauges Feature - "How much rain has fallen near me?"

## Overview

Add ability to query rainfall data based on user's geographic location (lat/lon) by extracting gauge coordinates from MCFCD FOPR Excel files.

## Data Source

**FOPR Excel Files**: `https://alert.fcd.maricopa.gov/alert/Rain/FOPR/{station_id}_FOPR.xlsx`

Example: https://alert.fcd.maricopa.gov/alert/Rain/FOPR/59700_FOPR.xlsx

### Excel Structure (Meta_Stats sheet)

The first sheet ("Meta_Stats") contains gauge metadata in a row-based format:

```
Row 0:  FCDMC Official Precipitation Record  |  Col 16: Updated: [DateTime]
Row 2:  Station Name: "Aztec Park"
Row 3:  Gage ID # History: "59700; 4695 prior to 2/20/2018"
Row 6:  Years Since Installation: 26.64 | as of: [DateTime]
Row 7:  Data Begins: [DateTime - e.g., 1998-02-13]
Row 8:  In or Nearest City/Town: "Scottsdale"
Row 10: Latitude: "33° 36' 36.2", 33.61006
Row 11: Longitude: "111° 51' 55.6", -111.86545
Row 12: Elevation: "1,465 ft."
Row 13: Location: "Near Thunderbird & Frank Lloyd Wright"
Row 14: Average Annual Precipitation for 26 Complete Years (in): | Col 3: 7.4803
Row 15: Data - Incomplete Months: "None"
Row 16: Data - Missing Months: "None"
Row 17: Remarks: "Records Good"
```

**Other Sheets Available**:
- **AnnualTables**: Water-year view with selectable year dropdown
- **DownTime**: Gauge outage history (dates, causes, impact assessment)
- **FREQ**: Maximum precipitation records by duration (15min/1hr/3hr/6hr/24hr) per year
- **FREQ_Plot**: Depth-duration plots for frequency analysis
- **WY-DD**: Water year statistics (days with rain per year)
- **YYYY sheets** (2024, 2023, etc.): Complete daily historical rainfall data back to 1998

**Data Priority for Extraction**:

#### High Priority (Essential - Extract Now):
1. **Latitude** (Row 10, Col 2): Decimal format, e.g., 33.61006
2. **Longitude** (Row 11, Col 2): Decimal format, e.g., -111.86545
3. **Data Begins** (Row 7, Col 1): Installation/start date - know historical data availability
4. **Average Annual Precipitation** (Row 14, Col 3): Long-term average for comparisons
5. **Average Annual Precipitation Sample Size** (Row 14, Col 0): Parse from label text "for X Complete Years" - e.g., 26
6. **Gage ID History** (Row 3, Col 1): Track gauge replacements/ID changes over time
7. **Remarks** (Row 17, Col 1): Data quality indicator (e.g., "Records Good", "Estimated")

#### Medium Priority (Nice to Have):
8. **Updated Timestamp** (Row 0, Col 16): Last FOPR file update - cache freshness indicator
9. **Years Since Installation** (Row 6, Col 1): Gauge operational age
10. **Incomplete/Missing Months** (Row 15-16, Col 1): Data quality flags

#### Low Priority (Future Features):
- **DownTime history**: Complex parsing, useful for reliability analysis
- **FREQ records**: Extreme event statistics (interesting but not essential)
- **Historical daily sheets**: Daily rainfall since 1998 - **massive feature for historical queries** (separate project)

## Implementation Plan

### 1. Database Schema Changes

**Migration**: `migrations/20250105000000_add_gauge_fopr_metadata.sql`

```sql
-- Enable PostgreSQL earthdistance extension for efficient geospatial queries
CREATE EXTENSION IF NOT EXISTS cube;
CREATE EXTENSION IF NOT EXISTS earthdistance;

-- Add geographic and metadata fields from FOPR Excel files
ALTER TABLE gauge_summaries
ADD COLUMN latitude DOUBLE PRECISION,
ADD COLUMN longitude DOUBLE PRECISION,
ADD COLUMN data_begins_date DATE,
ADD COLUMN average_annual_precipitation_inches DOUBLE PRECISION,
ADD COLUMN average_annual_precipitation_years INTEGER,
ADD COLUMN gage_id_history TEXT,
ADD COLUMN data_quality_remarks TEXT,
ADD COLUMN fopr_last_updated_at TIMESTAMPTZ;

-- GiST index for spatial queries using earthdistance
-- This enables fast bounding box queries for "nearby" searches
CREATE INDEX idx_gauge_earth ON gauge_summaries
USING gist(ll_to_earth(latitude, longitude));

-- Index for data quality filtering
CREATE INDEX idx_gauge_data_quality ON gauge_summaries(data_quality_remarks);
```

**Field Descriptions**:
- `latitude`, `longitude`: Geographic coordinates (decimal degrees)
- `data_begins_date`: When gauge started collecting data (installation date)
- `average_annual_precipitation_inches`: Long-term average for comparison/normalization (from FOPR)
- `average_annual_precipitation_years`: Number of complete years in the FOPR average (e.g., 26)
- `gage_id_history`: Track gauge ID changes (e.g., "59700; 4695 prior to 2/20/2018")
- `data_quality_remarks`: Status like "Records Good", "Estimated", etc.
- `fopr_last_updated_at`: When FOPR file was last updated (cache freshness)

**Note on Average Calculation**:
The `average_annual_precipitation_*` fields represent the **historical baseline** from FOPR based on complete water years. These are snapshots from the FOPR file and should NOT be updated with live readings. For real-time rolling averages, calculate dynamically from the `readings` table when needed.

### 2. Excel Fetcher/Parser

**New Module**: `src/fopr_fetcher.rs`

```rust
use calamine::{Reader, open_workbook_auto_from_rs, Data, Xlsx};
use chrono::{NaiveDate, DateTime, Utc};
use reqwest::Client;
use std::io::Cursor;

pub struct FoprMetadata {
    pub station_id: String,
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub data_begins_date: Option<NaiveDate>,
    pub average_annual_precipitation_inches: Option<f64>,
    pub average_annual_precipitation_years: Option<i32>,
    pub gage_id_history: Option<String>,
    pub data_quality_remarks: Option<String>,
    pub fopr_last_updated_at: Option<DateTime<Utc>>,
}

pub struct FoprFetcher {
    client: Client,
    base_url: String,
}

impl FoprFetcher {
    pub fn new(base_url: String) -> Self {
        Self {
            client: Client::new(),
            base_url,
        }
    }

    pub async fn fetch_metadata(&self, station_id: &str) -> Result<FoprMetadata, FetchError> {
        // 1. Download FOPR Excel file to memory
        let url = format!("{}/{}_FOPR.xlsx", self.base_url, station_id);
        let bytes = self.client.get(&url).send().await?.bytes().await?;

        // 2. Parse with calamine using spawn_blocking()
        let station_id_owned = station_id.to_string();
        let metadata = tokio::task::spawn_blocking(move || {
            parse_fopr_metadata(&bytes, &station_id_owned)
        }).await??;

        Ok(metadata)
    }
}

fn parse_fopr_metadata(bytes: &[u8], station_id: &str) -> Result<FoprMetadata, FetchError> {
    let cursor = Cursor::new(bytes);
    let mut workbook: Xlsx<_> = open_workbook_auto_from_rs(cursor)?;

    // Read Meta_Stats sheet (first sheet)
    let range = workbook.worksheet_range_at(0)
        .ok_or(FetchError::SheetNotFound)??;

    let mut metadata = FoprMetadata {
        station_id: station_id.to_string(),
        ..Default::default()
    };

    // Extract fields by row index
    // Row 0, Col 16: Updated timestamp
    if let Some(cell) = range.get_value((0, 16)) {
        metadata.fopr_last_updated_at = parse_datetime(cell);
    }

    // Row 3, Col 1: Gage ID History
    if let Some(Data::String(s)) = range.get_value((3, 1)) {
        metadata.gage_id_history = Some(s.clone());
    }

    // Row 7, Col 1: Data Begins date
    if let Some(cell) = range.get_value((7, 1)) {
        metadata.data_begins_date = parse_date(cell);
    }

    // Row 10, Col 2: Latitude (decimal)
    if let Some(cell) = range.get_value((10, 2)) {
        metadata.latitude = parse_float(cell);
    }

    // Row 11, Col 2: Longitude (decimal)
    if let Some(cell) = range.get_value((11, 2)) {
        metadata.longitude = parse_float(cell);
    }

    // Row 14, Col 0: Parse "Average Annual Precipitation for X Complete Years (in):"
    // Extract the sample size (X) using regex
    if let Some(Data::String(s)) = range.get_value((14, 0)) {
        metadata.average_annual_precipitation_years = parse_years_from_label(s);
    }

    // Row 14, Col 3: Average Annual Precipitation value
    if let Some(cell) = range.get_value((14, 3)) {
        metadata.average_annual_precipitation_inches = parse_float(cell);
    }

    // Row 17, Col 1: Remarks
    if let Some(Data::String(s)) = range.get_value((17, 1)) {
        metadata.data_quality_remarks = Some(s.clone());
    }

    Ok(metadata)
}

// Helper functions
fn parse_float(data: &Data) -> Option<f64> {
    match data {
        Data::Float(f) => Some(*f),
        Data::Int(i) => Some(*i as f64),
        _ => None,
    }
}

fn parse_date(data: &Data) -> Option<NaiveDate> {
    match data {
        Data::DateTime(dt) => dt.as_date(),
        _ => None,
    }
}

fn parse_datetime(data: &Data) -> Option<DateTime<Utc>> {
    match data {
        Data::DateTime(dt) => dt.as_datetime().map(|d| DateTime::from_naive_utc_and_offset(d, Utc)),
        _ => None,
    }
}

fn parse_years_from_label(label: &str) -> Option<i32> {
    // Parse "Average Annual Precipitation for 26 Complete Years (in):" -> 26
    // Regex: "for (\d+) Complete Years"
    let re = regex::Regex::new(r"for (\d+) Complete Years").ok()?;
    re.captures(label)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
}
```

**Note**: This parsing approach requires the `regex` crate. Add to `Cargo.toml`:
```toml
regex = "1"
```

**Parsing Logic**:
- Use `tokio::task::spawn_blocking()` since calamine is sync
- Open workbook from bytes (in-memory, no temp files)
- Read "Meta_Stats" sheet (first sheet, index 0)
- Extract data by row/column indices:
  - Row 0, Col 16: Updated timestamp
  - Row 3, Col 1: Gage ID history
  - Row 7, Col 1: Data begins date
  - Row 10, Col 2: Latitude (decimal)
  - Row 11, Col 2: Longitude (decimal)
  - Row 14, Col 0: Parse label with regex `for (\d+) Complete Years` → sample size
  - Row 14, Col 3: Average annual precipitation value
  - Row 17, Col 1: Data quality remarks
- Handle `Data::Float`, `Data::Int`, `Data::String`, `Data::DateTime` variants
- Return `None` for missing/malformed fields (graceful degradation)
- Regex parsing for sample size is fault-tolerant (returns `None` if pattern doesn't match)

### 3. FOPR Metadata Import Job

**New Binary**: `src/bin/import_fopr_metadata.rs`

A standalone binary that runs once to import FOPR metadata for all gauges:

```rust
use rain_tracker_service::{Config, FoprFetcher};
use rain_tracker_service::db::{GaugeRepository, connect_to_database};
use std::time::Duration;
use tokio::time::sleep;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing
    tracing_subscriber::fmt::init();

    // Load configuration
    let config = Config::from_env()?;

    // Connect to database
    let pool = connect_to_database(&config.database_url).await?;
    let gauge_repo = GaugeRepository::new(pool.clone());

    // Create FOPR fetcher
    let fopr_fetcher = FoprFetcher::new(config.fopr_base_url);

    // Fetch all gauges
    let gauges = gauge_repo.find_all().await?;
    tracing::info!("Found {} gauges to process", gauges.len());

    let mut success_count = 0;
    let mut error_count = 0;

    // Process each gauge
    for (idx, gauge) in gauges.iter().enumerate() {
        tracing::info!(
            "Processing gauge {}/{}: {}",
            idx + 1,
            gauges.len(),
            gauge.station_id
        );

        match fopr_fetcher.fetch_metadata(&gauge.station_id).await {
            Ok(metadata) => {
                match gauge_repo.update_fopr_metadata(&gauge.station_id, &metadata).await {
                    Ok(_) => {
                        success_count += 1;
                        tracing::info!("✓ Updated metadata for {}", gauge.station_id);
                    }
                    Err(e) => {
                        error_count += 1;
                        tracing::error!("✗ Failed to save metadata for {}: {}", gauge.station_id, e);
                    }
                }
            }
            Err(e) => {
                error_count += 1;
                tracing::warn!("✗ Failed to fetch FOPR for {}: {}", gauge.station_id, e);
            }
        }

        // Rate limiting: 1-2 second delay between requests
        sleep(Duration::from_millis(1500)).await;
    }

    tracing::info!(
        "Import complete: {} successful, {} errors",
        success_count,
        error_count
    );

    Ok(())
}
```

**Usage**:

```bash
# Run locally
export DATABASE_URL=postgres://postgres:password@localhost:5432/rain_tracker
export FOPR_BASE_URL=https://alert.fcd.maricopa.gov/alert/Rain/FOPR
cargo run --bin import-fopr-metadata

# Or via Docker
docker run --rm \
  -e DATABASE_URL=$DATABASE_URL \
  -e FOPR_BASE_URL=$FOPR_BASE_URL \
  ghcr.io/yourorg/rain-tracker-service:latest \
  ./import-fopr-metadata
```

**K8s Job** (one-time manual execution):

```yaml
# k8s/job-fopr-import.yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: fopr-metadata-import
  namespace: default
spec:
  template:
    spec:
      containers:
      - name: import
        image: ghcr.io/yourorg/rain-tracker-service:latest
        command: ["./import-fopr-metadata"]
        envFrom:
          - configMapRef:
              name: rain-tracker-config
          - secretRef:
              name: rain-tracker-db-secrets
        env:
          - name: FOPR_BASE_URL
            value: "https://alert.fcd.maricopa.gov/alert/Rain/FOPR"
      restartPolicy: OnFailure
      backoffLimit: 3
```

**K8s CronJob** (optional - for periodic updates):

```yaml
# k8s/cronjob-fopr-import.yaml
apiVersion: batch/v1
kind: CronJob
metadata:
  name: fopr-metadata-import
  namespace: default
spec:
  schedule: "0 2 1 * *"  # 2am on 1st day of each month
  successfulJobsHistoryLimit: 3
  failedJobsHistoryLimit: 3
  jobTemplate:
    spec:
      template:
        spec:
          containers:
          - name: import
            image: ghcr.io/yourorg/rain-tracker-service:latest
            command: ["./import-fopr-metadata"]
            envFrom:
              - configMapRef:
                  name: rain-tracker-config
              - secretRef:
                  name: rain-tracker-db-secrets
            env:
              - name: FOPR_BASE_URL
                value: "https://alert.fcd.maricopa.gov/alert/Rain/FOPR"
          restartPolicy: OnFailure
          backoffLimit: 3
```

**Run K8s Job Manually**:

```bash
# Create and run job
kubectl apply -f k8s/job-fopr-import.yaml

# Watch progress
kubectl logs -f job/fopr-metadata-import

# Check status
kubectl get jobs

# Clean up after success
kubectl delete job fopr-metadata-import
```

**Why a Job Instead of Persistent Scheduler**:

1. **FOPR metadata changes infrequently**: Lat/lon, installation dates, historical averages are mostly static
2. **Resource efficient**: No persistent background task consuming memory 24/7
3. **On-demand execution**: Run when needed (initial import, quarterly updates, etc.)
4. **Simpler architecture**: One less background task in main service to monitor
5. **Better isolation**: Import logic separate from API service
6. **Flexible scheduling**: Can be manual, CronJob, or event-triggered
7. **Easier testing**: Run locally or in CI without starting full service

### 4. Repository Layer

**Update**: `src/db/gauge_repository.rs`

Add methods:
```rust
pub async fn update_fopr_metadata(
    &self,
    station_id: &str,
    metadata: &FoprMetadata,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        r#"
        UPDATE gauge_summaries
        SET latitude = $2,
            longitude = $3,
            data_begins_date = $4,
            average_annual_precipitation_inches = $5,
            average_annual_precipitation_years = $6,
            gage_id_history = $7,
            data_quality_remarks = $8,
            fopr_last_updated_at = $9,
            updated_at = NOW()
        WHERE station_id = $1
        "#,
        station_id,
        metadata.latitude,
        metadata.longitude,
        metadata.data_begins_date,
        metadata.average_annual_precipitation_inches,
        metadata.average_annual_precipitation_years,
        metadata.gage_id_history.as_deref(),
        metadata.data_quality_remarks.as_deref(),
        metadata.fopr_last_updated_at,
    )
    .execute(&self.pool)
    .await?;

    Ok(())
}

pub async fn find_nearby_gauges(
    &self,
    user_lat: f64,
    user_lon: f64,
    radius_miles: f64,
    limit: i64,
) -> Result<Vec<(GaugeSummary, f64)>, sqlx::Error> {
    let radius_meters = radius_miles * 1609.34;

    // Note: SQLx can't handle earth_distance functions directly in query_as!
    // We need to use query! and manually map the results
    let results = sqlx::query!(
        r#"
        SELECT
            id,
            station_id,
            gauge_name,
            city_town,
            elevation_ft,
            general_location,
            msp_forecast_zone,
            rainfall_past_6h_inches,
            rainfall_past_24h_inches,
            latitude,
            longitude,
            data_begins_date,
            average_annual_precipitation_inches,
            average_annual_precipitation_years,
            gage_id_history,
            data_quality_remarks,
            fopr_last_updated_at,
            last_scraped_at,
            created_at,
            updated_at,
            earth_distance(
                ll_to_earth($1, $2),
                ll_to_earth(latitude, longitude)
            ) * 0.000621371 as "distance_miles!"
        FROM gauge_summaries
        WHERE latitude IS NOT NULL
          AND longitude IS NOT NULL
          AND earth_box(ll_to_earth($1, $2), $3) @> ll_to_earth(latitude, longitude)
        ORDER BY earth_distance(
            ll_to_earth($1, $2),
            ll_to_earth(latitude, longitude)
        )
        LIMIT $4
        "#,
        user_lat,
        user_lon,
        radius_meters,
        limit
    )
    .fetch_all(&self.pool)
    .await?;

    // Map results to (GaugeSummary, distance) tuples
    Ok(results.into_iter().map(|r| {
        (
            GaugeSummary {
                id: r.id,
                station_id: r.station_id,
                gauge_name: r.gauge_name,
                city_town: r.city_town,
                elevation_ft: r.elevation_ft,
                general_location: r.general_location,
                msp_forecast_zone: r.msp_forecast_zone,
                rainfall_past_6h_inches: r.rainfall_past_6h_inches,
                rainfall_past_24h_inches: r.rainfall_past_24h_inches,
                latitude: r.latitude,
                longitude: r.longitude,
                data_begins_date: r.data_begins_date,
                average_annual_precipitation_inches: r.average_annual_precipitation_inches,
                average_annual_precipitation_years: r.average_annual_precipitation_years,
                gage_id_history: r.gage_id_history,
                data_quality_remarks: r.data_quality_remarks,
                fopr_last_updated_at: r.fopr_last_updated_at,
                last_scraped_at: r.last_scraped_at,
                created_at: r.created_at,
                updated_at: r.updated_at,
            },
            r.distance_miles,
        )
    }).collect())
}
```

**How This Works**:

1. **Bounding Box Filter**: `earth_box(ll_to_earth($1, $2), $3) @> ll_to_earth(latitude, longitude)`
   - Fast GiST index lookup to eliminate distant gauges
   - Only calculates distance for gauges roughly within radius

2. **Distance Calculation**: `earth_distance(...) * 0.000621371`
   - Calculates great-circle distance in meters, converts to miles
   - Happens in PostgreSQL, not in application memory

3. **Sort and Limit**: `ORDER BY earth_distance(...) LIMIT $4`
   - Database handles sorting and pagination
   - Only transfers requested number of rows over network

**Performance Benefits**:
- ✅ **Minimal network transfer**: Only returns matching gauges (5-10 rows instead of 200)
- ✅ **Low memory usage**: App only loads results that match criteria
- ✅ **GiST index**: Spatial index makes bounding box query fast
- ✅ **Scalable**: Works efficiently even if gauge count grows to 1000+

**Accuracy**:
- PostgreSQL `earthdistance` assumes perfect sphere (not oblate spheroid)
- Accurate within ~0.5% for distances <100 miles
- More than sufficient for "rain near me" queries

### 5. ~~Haversine Distance Function~~ (Not Needed)

~~**New Module**: `src/geo.rs`~~

**This section is no longer needed** - distance calculations are handled by PostgreSQL `earthdistance` extension in the database query. No application-side distance calculation required.

### 6. Service Layer

**Update**: `src/services/gauge_service.rs`

```rust
pub async fn find_nearby_gauges(
    &self,
    user_lat: f64,
    user_lon: f64,
    radius_miles: f64,
    limit: Option<usize>,
) -> Result<Vec<NearbyGaugeInfo>, ServiceError> {
    // Validate parameters
    if !(-90.0..=90.0).contains(&user_lat) {
        return Err(ServiceError::InvalidParameter("Latitude must be between -90 and 90".into()));
    }
    if !(-180.0..=180.0).contains(&user_lon) {
        return Err(ServiceError::InvalidParameter("Longitude must be between -180 and 180".into()));
    }
    if radius_miles <= 0.0 || radius_miles > 100.0 {
        return Err(ServiceError::InvalidParameter("Radius must be between 0 and 100 miles".into()));
    }

    // Call repository (database does all the heavy lifting)
    let limit = limit.unwrap_or(5).min(50) as i64; // Max 50 results
    let results = self.gauge_repository
        .find_nearby_gauges(user_lat, user_lon, radius_miles, limit)
        .await?;

    // Optionally fetch latest reading for each gauge
    let mut nearby_gauges = Vec::new();
    for (gauge, distance) in results {
        let latest_reading = self.reading_repository
            .find_latest_by_station_id(&gauge.station_id)
            .await
            .ok();

        nearby_gauges.push(NearbyGaugeInfo {
            gauge,
            distance_miles: distance,
            latest_reading,
        });
    }

    Ok(nearby_gauges)
}
```

**Benefits of This Approach**:
- Service layer stays thin - just validation and coordination
- All expensive computation (distance, filtering, sorting) happens in database
- Can easily add features like fetching latest readings without impacting performance

**New Model**: `src/db/models.rs`

```rust
#[derive(Debug, Clone, Serialize, Deserialize, ToSchema)]
pub struct NearbyGaugeInfo {
    pub gauge: GaugeSummary,
    pub distance_miles: f64,
    pub latest_reading: Option<Reading>,
}
```

### 7. API Layer

**New Endpoint**: `GET /api/v1/gauges/nearby`

**Query Parameters**:
- `lat` (required): User latitude (decimal)
- `lon` (required): User longitude (decimal)
- `radius_miles` (optional, default: 10): Search radius
- `limit` (optional, default: 5): Max results

**Example Request**:
```
GET /api/v1/gauges/nearby?lat=33.4484&lon=-112.0740&radius_miles=15&limit=10
```

**Example Response**:
```json
{
  "user_location": {
    "latitude": 33.4484,
    "longitude": -112.0740
  },
  "radius_miles": 15.0,
  "gauges": [
    {
      "gauge": {
        "station_id": "59700",
        "gauge_name": "Aztec Park",
        "city_town": "Scottsdale",
        "latitude": 33.61006,
        "longitude": -111.86545,
        "elevation_ft": 1465,
        "general_location": "Near Thunderbird & Frank Lloyd Wright",
        "rainfall_past_6h_inches": 0.0,
        "rainfall_past_24h_inches": 0.25
      },
      "distance_miles": 12.3,
      "latest_reading": {
        "id": 12345,
        "station_id": "59700",
        "reading_date": "2025-01-19T10:00:00Z",
        "rainfall_inches": 0.25
      }
    }
  ]
}
```

**Handler**: `src/api.rs`

```rust
#[utoipa::path(
    get,
    path = "/api/v1/gauges/nearby",
    params(
        ("lat" = f64, Query, description = "User latitude"),
        ("lon" = f64, Query, description = "User longitude"),
        ("radius_miles" = Option<f64>, Query, description = "Search radius in miles (default: 10)"),
        ("limit" = Option<usize>, Query, description = "Maximum number of results (default: 5)")
    ),
    responses(
        (status = 200, description = "Nearby gauges found", body = NearbyGaugesResponse),
        (status = 400, description = "Invalid parameters")
    ),
    tag = "gauges"
)]
async fn get_nearby_gauges(
    Query(params): Query<NearbyGaugesParams>,
    State(gauge_service): State<Arc<GaugeService>>,
) -> Result<Json<NearbyGaugesResponse>, StatusCode> {
    match gauge_service.find_nearby_gauges(
        params.lat,
        params.lon,
        params.radius_miles.unwrap_or(10.0),
        params.limit.map(|l| l as usize),
    ).await {
        Ok(gauges) => Ok(Json(NearbyGaugesResponse {
            user_location: Location {
                latitude: params.lat,
                longitude: params.lon,
            },
            radius_miles: params.radius_miles.unwrap_or(10.0),
            gauges,
        })),
        Err(e) => {
            tracing::error!("Failed to find nearby gauges: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}
```

### 8. Configuration

**Update**: `src/config.rs`

```rust
pub struct Config {
    // ... existing fields
    pub fopr_base_url: String,
}
```

**Update**: `.env.example`

```
FOPR_BASE_URL=https://alert.fcd.maricopa.gov/alert/Rain/FOPR
```

**Note**: No `FOPR_FETCH_INTERVAL_HOURS` needed since we're using a one-time K8s job instead of a persistent scheduler.

## Testing Strategy

### Unit Tests

1. **FOPR parser** (`src/fopr_fetcher.rs`):
   - Test with real downloaded Excel file
   - Test with malformed data
   - Test missing lat/lon
   - Test regex parsing for sample size

### Integration Tests

1. **Database operations** (`tests/integration_test.rs`):
   - Test `update_fopr_metadata()`
   - Test `find_nearby_gauges()` with PostgreSQL earthdistance
   - Verify GiST index is used (EXPLAIN ANALYZE)
   - Test distance calculations are accurate

2. **Import job** (`src/bin/import_fopr_metadata.rs`):
   - Run locally against test database
   - Verify all fields populated correctly
   - Test rate limiting behavior

3. **API endpoint** (`http/api-tests.http`):
   ```http
   ### Get nearby gauges
   GET {{baseUrl}}/api/v1/gauges/nearby?lat=33.4484&lon=-112.0740&radius_miles=20
   Accept: application/json
   ```

## Rollout Plan

### Phase 1: Data Collection (Non-Breaking)
1. Add migration for FOPR metadata columns (8 new fields)
2. Implement FoprFetcher module
3. Implement `import-fopr-metadata` binary
4. Add K8s job manifest
5. Run import job to populate metadata (one-time)
6. Verify data quality (check for missing lat/lon, parse errors)

### Phase 2: API Endpoint (Non-Breaking)
1. Implement repository method using `earthdistance`
2. Add service layer with validation
3. Add API endpoint handler
4. Update OpenAPI docs
5. Test with various locations and radii

### Phase 3: Monitoring & Maintenance
1. Monitor query performance (should be <10ms for typical queries)
2. Set up K8s CronJob for periodic updates (optional - monthly/quarterly)
3. Add monitoring/alerting for import job failures
4. Verify GiST index is being used efficiently

## Edge Cases & Considerations

1. **Missing Coordinates**: Some gauges may not have FOPR files or lat/lon
   - Handle gracefully, exclude from nearby queries
   - Log warning for manual investigation

2. **Coordinate Validation**: Ensure lat/lon are valid
   - Latitude: -90 to 90
   - Longitude: -180 to 180
   - Maricopa County is roughly: 33°N, 112°W

3. **FOPR File Unavailability**: HTTP errors, parse errors
   - Retry logic with exponential backoff
   - Don't fail entire batch if one gauge fails
   - Log errors for monitoring

4. **Distance Accuracy**: PostgreSQL `earthdistance` assumes spherical Earth
   - Accurate within ~0.5% for distances <100 miles
   - More than sufficient for "near me" queries
   - Uses mean Earth radius of 6371 km

5. **Rate Limiting**: Downloading Excel files for all gauges
   - Add delay between requests (1-2 seconds)
   - Respect MCFCD servers

6. **Excel Format Changes**: MCFCD could change Excel structure
   - Add validation/error handling
   - Log warnings if structure differs
   - Fallback to parsing by cell labels

7. **PostgreSQL Extensions**: `cube` and `earthdistance` must be available
   - Extensions are standard PostgreSQL (not third-party)
   - May require superuser to enable: `CREATE EXTENSION`
   - Include in migrations so they auto-install on new databases

## Alternative: Geocoding Approach

If FOPR files become unavailable or unreliable, could geocode the `city_town` + `general_location` text fields:

**Services**:
- Nominatim (OpenStreetMap) - Free, requires attribution
- Google Maps Geocoding API - Paid, accurate
- Mapbox Geocoding - Paid, good free tier

**Example**:
```
"Scottsdale" + "Near Thunderbird & Frank Lloyd Wright"
→ Geocoding API
→ lat: 33.61006, lon: -111.86545
```

**Pros**: Doesn't require Excel parsing
**Cons**: External API dependency, rate limits, costs

## Future Enhancements

1. **Reverse Geocoding**: "How much rain in Scottsdale?" → convert city name to lat/lon
2. **Bounding Box Queries**: "Rain in Phoenix metro area"
3. **Time-Based Nearby Queries**: "Rainfall near me in last 24 hours"
4. **Map Visualization**: Frontend showing gauges on a map
5. **Push Notifications**: "Heavy rain detected near your location"
6. **Historical Nearby Data**: "Average rainfall near me last 10 years"

## Effort Estimate

- **Phase 1 (Data Collection)**: 2-3 days
  - Migration: 1.5 hours (add 8 columns + enable earthdistance extensions + GiST index)
  - FoprFetcher: 6-8 hours (parsing 8 fields with regex)
  - Import binary: 2-3 hours (standalone job with rate limiting & logging)
  - K8s manifests: 1 hour (Job + CronJob)
  - Testing: 3-4 hours (validate all fields, test import job)

- **Phase 2 (API Endpoint)**: 2-3 days
  - Repository method: 3-4 hours (complex SQL with earthdistance, manual field mapping)
  - Service layer: 2-3 hours (validation and coordination)
  - API handler: 2-3 hours
  - OpenAPI docs: 1 hour
  - Testing: 3-4 hours (test accuracy, verify index usage)

- **Phase 3 (Polish)**: 1-2 days
  - Error handling: 2-3 hours
  - Logging/monitoring: 2-3 hours
  - Performance testing: 2-3 hours
  - Documentation: 1-2 hours

**Total**: 5-8 days (1-1.5 weeks)

**Notes**:
- Using PostgreSQL `earthdistance` adds ~1 hour to repository implementation but provides significant performance and scalability benefits
- Using a K8s job instead of persistent scheduler saves implementation time (~1 hour) and reduces ongoing maintenance complexity
- The database-side approach is more efficient (less network traffic, less memory) and scales better

## Dependencies

- ✅ `calamine` - Already added for Excel parsing
- ✅ `reqwest` - Already in use for HTTP requests
- ✅ `tokio` - Already in use for async runtime
- ✅ `sqlx` - Already in use for database
- ⚠️ `regex` - **New dependency required** for parsing sample size from label text

**One new dependency**: `regex = "1"` for parsing "for X Complete Years" pattern

## Success Criteria

1. ✅ All gauges have lat/lon populated (>90% coverage)
2. ✅ Import job completes successfully with <10% error rate
3. ✅ API endpoint returns accurate nearby gauges
4. ✅ Distance calculations accurate within 1% of known values
5. ✅ Response time < 200ms for typical queries
6. ✅ OpenAPI documentation updated
7. ✅ All tests passing
8. ✅ K8s job can be run manually and scheduled via CronJob

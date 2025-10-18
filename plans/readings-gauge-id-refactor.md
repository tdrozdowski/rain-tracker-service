# Readings Endpoint Refactor: Add Gauge ID Parameter

## Overview

This refactoring adds a gauge ID (station_id) parameter to the readings endpoints, changing the URL structure from global readings queries to gauge-specific queries.

## Current State

**Current Endpoints:**
- `GET /api/v1/readings/water-year/{year}` - Returns all readings for all gauges for a water year
- `GET /api/v1/readings/calendar-year/{year}` - Returns all readings for all gauges for a calendar year
- `GET /api/v1/readings/latest` - Returns the latest reading from any gauge

**Problem:** The current endpoints return data for all gauges combined, which doesn't align with the multi-gauge nature of the system. Now that we have gauge-specific endpoints and data, readings should be scoped to individual gauges.

## Target State

**New Endpoints:**
- `GET /api/v1/readings/{gauge_id}/water-year/{year}` - Returns readings for a specific gauge for a water year
- `GET /api/v1/readings/{gauge_id}/calendar-year/{year}` - Returns readings for a specific gauge for a calendar year
- `GET /api/v1/readings/{gauge_id}/latest` - Returns the latest reading for a specific gauge

**Benefits:**
1. Consistent with RESTful design (resource hierarchy: gauges → readings)
2. Allows clients to query readings for specific gauges
3. Better performance by filtering at the database level
4. Aligns with the gauge-centric data model
5. Eliminates non-deterministic behavior when multiple gauges have readings at the same timestamp

## Implementation Plan

### 1. API Layer (`src/api.rs`)

**Changes:**
- Update route from `/readings/water-year/{year}` to `/readings/{station_id}/water-year/{year}`
- Update route from `/readings/calendar-year/{year}` to `/readings/{station_id}/calendar-year/{year}`
- Modify handler signatures to extract both `station_id` and `year` from path parameters
- Pass `station_id` to service layer

**Code Changes:**
```rust
// Before
.route("/readings/water-year/{year}", get(get_water_year))
.route("/readings/calendar-year/{year}", get(get_calendar_year))

// After
.route("/readings/{station_id}/water-year/{year}", get(get_water_year))
.route("/readings/{station_id}/calendar-year/{year}", get(get_calendar_year))

// Handler signature changes
async fn get_water_year(
    State(state): State<AppState>,
    Path((station_id, year)): Path<(String, i32)>,
) -> Result<Json<crate::db::WaterYearSummary>, StatusCode>

async fn get_calendar_year(
    State(state): State<AppState>,
    Path((station_id, year)): Path<(String, i32)>,
) -> Result<Json<crate::db::CalendarYearSummary>, StatusCode>
```

### 2. Service Layer (`src/services/reading_service.rs`)

**Changes:**
- Add `station_id: &str` parameter to `get_water_year_summary()`
- Add `station_id: &str` parameter to `get_calendar_year_summary()`
- Pass `station_id` through to repository layer

**Method Signatures:**
```rust
// Before
pub async fn get_water_year_summary(&self, water_year: i32) -> Result<WaterYearSummary, DbError>
pub async fn get_calendar_year_summary(&self, year: i32) -> Result<CalendarYearSummary, DbError>

// After
pub async fn get_water_year_summary(&self, station_id: &str, water_year: i32) -> Result<WaterYearSummary, DbError>
pub async fn get_calendar_year_summary(&self, station_id: &str, year: i32) -> Result<CalendarYearSummary, DbError>
```

### 3. Repository Layer (`src/db/reading_repository.rs`)

**Changes:**
- Add `station_id: &str` parameter to `find_by_date_range()`
- Update SQL query to include `WHERE station_id = $3` filter
- Update bind parameters

**SQL Query Changes:**
```sql
-- Before
SELECT * FROM readings
WHERE reading_datetime >= $1 AND reading_datetime < $2
ORDER BY reading_datetime DESC

-- After
SELECT * FROM readings
WHERE reading_datetime >= $1 AND reading_datetime < $2 AND station_id = $3
ORDER BY reading_datetime DESC
```

### 4. Testing Updates

#### HTTP Tests (`http/api-tests.http`)
Update all test cases to include a gauge ID. Use `59700` as the primary test gauge ID.

**Example Changes:**
```http
# Before
GET {{baseUrl}}/api/v1/readings/water-year/2025

# After
GET {{baseUrl}}/api/v1/readings/59700/water-year/2025
```

**New Test Cases to Add:**
- Test with invalid gauge ID (should return 200 with empty data)
- Test with gauge ID that exists but has no readings for the specified year
- Test multiple different gauge IDs

#### Unit Tests (`src/services/reading_service.rs`)
- Update test mocks to include `station_id` parameter
- Verify date range calculations still work correctly

#### Integration Tests
- Update integration tests to use gauge-specific endpoints
- Test with multiple gauges to ensure proper isolation

### 5. Documentation Updates

#### README.md
Update the API endpoints section:

```markdown
### Get Water Year Readings
GET /api/v1/readings/{gauge_id}/water-year/{year}

Returns all readings for a specific gauge for a water year (Oct 1 of year-1 through Sep 30 of year).

Example: `GET /api/v1/readings/59700/water-year/2025` returns readings for gauge 59700 from Oct 1, 2024 to Sep 30, 2025.

### Get Calendar Year Readings
GET /api/v1/readings/{gauge_id}/calendar-year/{year}

Returns all readings for a specific gauge for a calendar year (Jan 1 through Dec 31).

Example: `GET /api/v1/readings/59700/calendar-year/2025` returns readings for gauge 59700 from Jan 1, 2025 to Dec 31, 2025.
```

Add recent changes section documenting:
- New `/api/v1/gauges` endpoint for listing all gauges with pagination
- New `/api/v1/gauges/{station_id}` endpoint for getting specific gauge details
- Refactored readings endpoints to be gauge-specific

### 6. SQLx Query Preparation

After all code changes:
```bash
cargo sqlx prepare
```

This will update `.sqlx/query-*.json` files with the new query metadata for offline compilation.

## Migration Strategy

**Breaking Change:** This is a breaking API change. Existing clients using the old endpoints will need to be updated.

**Recommended Approach:**
1. Deploy these changes to a development/staging environment first
2. Update any client applications or scripts
3. Verify all functionality works correctly
4. Deploy to production
5. Update API documentation

**Alternative (if backwards compatibility needed):**
- Keep old endpoints but mark as deprecated
- Have them return 301 redirects or 410 Gone status
- Remove after a deprecation period

For this project, we're proceeding with the breaking change since it's in active development.

## Verification Checklist

- [x] All route definitions updated in `src/api.rs`
- [x] Handler signatures updated to extract `station_id`
- [x] Service layer methods accept `station_id` parameter
- [x] Repository layer filters by `station_id`
- [x] SQL queries updated and verified
- [x] HTTP test file updated with new endpoints
- [x] Unit tests pass: `cargo test --lib`
- [x] Integration tests fixed and updated
- [x] SQLx offline mode updated: `cargo sqlx prepare`
- [x] SQLx check passes: `cargo check`
- [ ] Manual testing with HTTP client confirms endpoints work
- [x] README.md updated with new endpoint documentation
- [x] README.md includes recent changes section
- [x] Latest reading endpoint also requires gauge ID
- [x] Health check endpoint simplified (removed latest_reading field)

## Additional Change: Latest Reading Endpoint

After initial implementation, we identified an issue with the `/api/v1/readings/latest` endpoint:

**Issue**: The endpoint ordered by `reading_datetime DESC` without filtering by gauge, causing non-deterministic behavior when multiple gauges have readings at the same timestamp.

**Solution**: Changed endpoint to require gauge ID: `/api/v1/readings/{gauge_id}/latest`

**Additional Changes Made**:
1. Updated `find_latest()` in repository to filter by `station_id`
2. Updated `get_latest_reading()` in service to accept `station_id` parameter
3. Updated `get_latest()` handler in API to extract `station_id` from path
4. Simplified health check endpoint to remove `latest_reading` field (now only returns status)
5. Updated integration tests to pass `station_id` to all reading queries
6. Updated HTTP tests for latest reading endpoint

## Files Modified

1. `src/api.rs` - Route definitions, handler signatures, and health check simplification
2. `src/services/reading_service.rs` - Service method signatures
3. `src/db/reading_repository.rs` - Repository method and SQL queries
4. `tests/integration_test.rs` - Integration test fixes
5. `http/api-tests.http` - All test endpoints
6. `README.md` - API documentation and recent changes
7. `.sqlx/query-*.json` - SQLx metadata (via `cargo sqlx prepare`)

## Expected Outcomes

After this refactoring:
- Clients can query readings for specific gauges
- Better database performance with gauge-specific queries
- More RESTful API design
- Consistent with the multi-gauge nature of the system
- Foundation for future gauge-specific features
- Eliminated non-deterministic behavior in latest reading queries
- Simplified health check endpoint focused on service status only

## Implementation Status

**Status**: ✅ COMPLETE

All planned changes have been implemented and tested:
- All three readings endpoints now require gauge ID
- Health check endpoint simplified
- Integration tests updated
- HTTP tests updated
- Documentation updated
- SQLx metadata updated
- Unit tests passing
- Compilation successful

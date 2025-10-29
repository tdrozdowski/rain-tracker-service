# Repository Business Logic Audit

**Date**: 2025-10-19
**Status**: Analysis Complete
**Priority**: Medium

## Executive Summary

Audit of all repository files to identify business logic violations. Found **1 repository with significant issues** and **2 repositories following best practices**.

### Results Overview

| Repository | Status | Business Logic Found | Action Required |
|------------|--------|---------------------|-----------------|
| `MonthlyRainfallRepository` | ❌ **VIOLATION** | Yes - Significant | **Refactor Required** |
| `ReadingRepository` | ✅ Clean | None | No action |
| `GaugeRepository` | ✅ Clean | None | No action |

## Detailed Analysis

### 1. MonthlyRainfallRepository ❌ VIOLATION

**File**: `src/db/monthly_rainfall_repository.rs`

**Severity**: HIGH - Contains multiple business logic violations

#### Business Logic Found

**Lines 32-56: Aggregate Calculations**
```rust
// ❌ BUSINESS LOGIC IN REPOSITORY
let total_rainfall: f64 = readings.iter().map(|r| r.incremental_inches).sum();
let reading_count = readings.len() as i32;

let first_reading_date = readings
    .iter()
    .min_by_key(|r| r.reading_datetime)
    .map(|r| r.reading_datetime);

let last_reading_date = readings
    .iter()
    .max_by_key(|r| r.reading_datetime)
    .map(|r| r.reading_datetime);

let min_cumulative = readings
    .iter()
    .map(|r| r.cumulative_inches)
    .min_by(|a, b| a.partial_cmp(b).unwrap())
    .unwrap_or(0.0);

let max_cumulative = readings
    .iter()
    .map(|r| r.cumulative_inches)
    .max_by(|a, b| a.partial_cmp(b).unwrap())
    .unwrap_or(0.0);
```

**Lines 158-175: Date/Time Manipulation**
```rust
// ❌ BUSINESS LOGIC IN REPOSITORY
let start_date = chrono::NaiveDate::from_ymd_opt(year, month as u32, 1)
    .unwrap()
    .and_hms_opt(0, 0, 0)
    .unwrap();
let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);

let end_date = if month == 12 {
    chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
} else {
    chrono::NaiveDate::from_ymd_opt(year, month as u32 + 1, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap()
};
let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);
```

#### Violations Identified

1. **Aggregate Calculations**: Summing, counting, min/max operations on collections
2. **Domain Logic**: Determining month boundaries, handling December rollover
3. **Data Transformation**: Converting readings to summary statistics
4. **Coordination**: `recalculate_monthly_summary` orchestrates multiple operations

#### Impact

- **Testability**: Cannot unit test calculations without database
- **Reusability**: Logic cannot be reused by other features (API, imports, schedulers)
- **Maintainability**: Changes to calculation logic require database setup
- **Separation of Concerns**: Repository has two responsibilities (data access + business logic)

#### Recommendation

**REFACTOR REQUIRED** - See `plans/monthly-rainfall-service-refactor.md` for detailed plan.

---

### 2. ReadingRepository ✅ CLEAN

**File**: `src/db/reading_repository.rs`

**Status**: Excellent - Follows best practices

#### What It Does Right

✅ **Pure Data Access** (Lines 20-56)
```rust
pub async fn insert_readings(&self, readings: &[RainReading]) -> Result<usize, DbError> {
    // Only SQL operations - no business logic
    let mut tx = self.pool.begin().await?;

    for reading in readings {
        sqlx::query!(/* ... */).execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(inserted)
}
```

✅ **Generic Query Methods** (Lines 61-90)
```rust
pub async fn find_by_date_range(
    &self,
    station_id: &str,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Result<Vec<Reading>, DbError> {
    // Simple SQL query - no date calculation logic
    // Caller provides start/end dates
    sqlx::query_as!(/* ... */).fetch_all(&self.pool).await
}
```

✅ **Clean Comment** (Line 59)
```rust
/// Generic query to find readings within a date range for a specific gauge
/// Business logic for water years, calendar years, etc. should be in service layer
```
This comment explicitly states the separation of concerns! 🎉

✅ **Simple CRUD** (Lines 94-119)
```rust
pub async fn find_latest(&self, station_id: &str) -> Result<Option<Reading>, DbError> {
    // Just SQL - no logic
    sqlx::query_as!(/* ... */).fetch_optional(&self.pool).await
}
```

#### Why It's Good

- **Single Responsibility**: Only handles database operations
- **Flexible**: Generic methods can be used by any service
- **Testable**: Service layer tests business logic without database
- **Maintainable**: Changes to business rules don't touch repository
- **Follows Existing Patterns**: Matches `ReadingService` design

#### No Action Required

This repository is an excellent example of clean architecture. Use as a reference for refactoring `MonthlyRainfallRepository`.

---

### 3. GaugeRepository ✅ CLEAN

**File**: `src/db/gauge_repository.rs`

**Status**: Good - Follows best practices

#### What It Does Right

✅ **Batch Operations** (Lines 18-65)
```rust
pub async fn upsert_summaries(&self, summaries: &[FetchedGauge]) -> Result<usize, DbError> {
    let mut tx = self.pool.begin().await?;

    for summary in summaries {
        sqlx::query!(/* upsert SQL */).execute(&mut *tx).await?;
    }

    tx.commit().await?;
    Ok(upserted)
}
```
- Transaction management: Good ✅
- No data transformation: Good ✅
- Receives pre-structured data: Good ✅

✅ **Simple CRUD** (Lines 68-131)
```rust
pub async fn count(&self) -> Result<usize, DbError>
pub async fn find_paginated(&self, offset: i64, limit: i64) -> Result<Vec<GaugeSummary>, DbError>
pub async fn find_by_id(&self, station_id: &str) -> Result<Option<GaugeSummary>, DbError>
```
- All methods are simple SQL queries
- No business logic
- No data manipulation

#### Why It's Good

- **Pure Data Access**: Only SQL operations
- **Transaction Handling**: Proper transaction management for batch operations
- **Generic Methods**: Offset/limit calculated by service layer
- **Clean Separation**: Pairs well with `GaugeService`

#### No Action Required

This repository correctly delegates business logic to `GaugeService` (pagination metadata calculation, etc.).

---

## Architecture Pattern Analysis

### Current Service Layer (For Comparison)

**ReadingService** (`src/services/reading_service.rs`)

✅ **Proper Business Logic in Service**:
- Lines 38-45: Calculating total rainfall/readings from monthly summaries
- Lines 48-59: Water year date range calculation
- Lines 74-78: Year-to-date rainfall calculation
- Lines 88: Building monthly summaries with cumulative YTD
- Lines 108-122: Water year date range logic
- Lines 124-138: Calendar year date range logic
- Lines 140-173: Building monthly summaries with cumulative calculations
- Lines 175-192: Month name mapping
- Lines 195-204: Water year determination logic

**GaugeService** (`src/services/gauge_service.rs`)

✅ **Proper Business Logic in Service**:
- Lines 68-70: Pagination metadata calculation (total_pages, has_next, has_prev)
- Lines 72: Aggregating last_scraped_at across gauges
- Lines 24-30: Offset/limit calculation logic

### Pattern to Follow

The existing services demonstrate the **correct pattern**:

```
Service Layer (Business Logic)
    ├─ Date range calculations
    ├─ Aggregations and summations
    ├─ Data transformations
    ├─ Orchestration (calling multiple repos)
    └─ Calls ↓
Repository Layer (Data Access)
    ├─ SQL queries only
    ├─ Transaction management
    └─ Simple CRUD operations
```

`MonthlyRainfallRepository` violates this pattern by putting business logic in the repository layer.

---

## Refactoring Priority

### High Priority (Do First)

1. **MonthlyRainfallRepository** → **MonthlyRainfallService**
   - Most significant violations
   - Blocks clean historical data import
   - Detailed plan already created: `plans/monthly-rainfall-service-refactor.md`

### Medium Priority (Future Consideration)

None identified - other repositories are clean.

### Low Priority (No Action Needed)

- `ReadingRepository` ✅
- `GaugeRepository` ✅

---

## Refactoring Plan Summary

### What Needs to Happen

**Create**: `src/services/monthly_rainfall_service.rs`

**Extract from Repository**:
- Aggregate calculation logic (lines 32-56)
- Date range calculation logic (lines 158-175)
- Orchestration logic (recalculate method)

**Update Repository to**:
- Accept pre-calculated aggregates instead of raw readings
- Remove date calculation logic
- Keep only SQL operations

**Pattern to Follow**:
- Use `ReadingService` as reference (excellent separation)
- Use `GaugeService` as reference (clean delegation)

### Estimated Effort

- **Development**: 4-6 hours
- **Testing**: 2-3 hours
- **Review**: 1-2 hours
- **Total**: ~1 day

See `plans/monthly-rainfall-service-refactor.md` for detailed implementation plan.

---

## Benefits of Refactoring

### Code Quality
- ✅ Clean architecture (proper layer separation)
- ✅ Single Responsibility Principle
- ✅ Testable business logic (without database)

### Maintainability
- ✅ Clear boundaries (easy to understand)
- ✅ Easy to modify (change logic without touching SQL)
- ✅ Easy to debug (isolate issues to specific layer)

### Extensibility
- ✅ Reusable calculations (API, schedulers, imports all use same service)
- ✅ Can add caching at service layer
- ✅ Can add validation at service layer
- ✅ Can add event publishing at service layer

### Historical Data Import
The upcoming historical data import feature (see `plans/historical-data-import.md`) will benefit significantly from this refactor:
- Bulk recalculation will be easier
- Business logic can be reused for import validation
- Service layer can coordinate between import and normal operations

---

## Recommendations

### Immediate Actions

1. ✅ **Approve refactoring plan** for `MonthlyRainfallRepository`
2. ✅ **Schedule refactor** before historical data import work
3. ✅ **Use incremental migration** (low risk, reviewable)

### Long-term Guidelines

**When adding new repositories**:
1. Keep them simple (CRUD only)
2. No business logic in repositories
3. Use existing repositories as examples (`ReadingRepository`, `GaugeRepository`)

**When adding new services**:
1. Put all business logic in services
2. Use existing services as examples (`ReadingService`, `GaugeService`)
3. Write unit tests for business logic

**Code Review Checklist**:
- [ ] Repository contains only SQL queries
- [ ] No calculations in repository methods
- [ ] No date/time manipulation in repository
- [ ] No orchestration logic in repository
- [ ] Business logic is in service layer
- [ ] Service has unit tests (without database)

---

## Success Criteria

After refactoring `MonthlyRainfallRepository`:

- [ ] All repositories contain only SQL operations
- [ ] All business logic is in service layer
- [ ] Unit tests exist for calculation logic
- [ ] Integration tests pass
- [ ] CI pipeline passes (`make ci-check`)
- [ ] No performance regression
- [ ] Code coverage maintained or improved

---

## Conclusion

**Good News**: 2 out of 3 repositories already follow best practices! ✅

**Action Required**: 1 repository needs refactoring to match the clean pattern established by the others.

The refactoring work is well-scoped and has a clear path forward. The existing `ReadingService` and `GaugeService` provide excellent examples to follow.

**Recommendation**: Proceed with `MonthlyRainfallRepository` refactor using the detailed plan in `plans/monthly-rainfall-service-refactor.md`.

---

## Appendix: Repository Method Summary

### MonthlyRainfallRepository (132 lines)
- `upsert_monthly_summary()` - ❌ Contains business logic (lines 32-56)
- `get_calendar_year_summaries()` - ✅ Clean SQL query
- `get_water_year_summaries()` - ✅ Clean SQL query
- `recalculate_monthly_summary()` - ❌ Contains business logic (lines 158-175)

### ReadingRepository (120 lines)
- `insert_readings()` - ✅ Clean SQL operations
- `find_by_date_range()` - ✅ Generic query, no business logic
- `find_latest()` - ✅ Simple CRUD

### GaugeRepository (132 lines)
- `upsert_summaries()` - ✅ Clean batch operation
- `count()` - ✅ Simple query
- `find_paginated()` - ✅ Generic query, offset/limit from caller
- `find_by_id()` - ✅ Simple CRUD

---

## References

- Clean Architecture (Robert C. Martin)
- SOLID Principles (Single Responsibility, Dependency Inversion)
- Repository Pattern (Martin Fowler)
- Project architecture guidelines: `CLAUDE.md`
- Existing service examples: `src/services/reading_service.rs`, `src/services/gauge_service.rs`

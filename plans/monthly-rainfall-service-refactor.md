# Monthly Rainfall Service Refactor Plan

**Date**: 2025-10-19
**Status**: Proposed
**Priority**: Medium

## Problem Statement

The `MonthlyRainfallRepository` (src/db/monthly_rainfall_repository.rs) contains significant business logic that violates the single responsibility principle and clean architecture patterns. Specifically:

### Business Logic in Repository Layer (Lines 32-56)
- **Aggregate calculations**: Summing incremental rainfall, counting readings
- **Data transformations**: Finding min/max cumulative values, first/last reading dates
- **Date/time manipulation**: Calculating month boundaries for recalculation (lines 158-175)

### Architectural Issues
1. **Tight coupling**: Business logic is tightly coupled to database operations
2. **Testability**: Difficult to unit test calculations without database
3. **Reusability**: Calculation logic cannot be reused outside repository context
4. **Layering violation**: Repository should only handle data access, not business rules

## Current Architecture

```
API Layer (handlers)
    ↓
Repository Layer (MonthlyRainfallRepository)
    ├─ Business Logic (aggregate calculations) ❌ WRONG LAYER
    └─ Database Operations (SQL queries)
```

## Target Architecture

```
API Layer (handlers)
    ↓
Service Layer (MonthlyRainfallService) ✅ NEW
    ├─ Business Logic (aggregate calculations)
    ├─ Orchestration (coordinate multiple repositories)
    └─ Uses ↓
Repository Layer (MonthlyRainfallRepository)
    └─ Database Operations (SQL queries only)
```

## Refactoring Plan

### Phase 1: Create Service Layer

**1.1 Create MonthlyRainfallService**

Location: `src/services/monthly_rainfall_service.rs`

Responsibilities:
- Calculate monthly aggregates from readings (extract from lines 32-56)
- Orchestrate upsert operations
- Handle water year vs calendar year logic
- Calculate date ranges for recalculation (extract from lines 158-175)

**1.2 Update services module declaration**

File: `src/services.rs`

Add:
```rust
pub mod monthly_rainfall_service;
pub use monthly_rainfall_service::MonthlyRainfallService;
```

### Phase 2: Extract Business Logic

**2.1 Create calculation methods in service**

Extract to `MonthlyRainfallService`:

```rust
// Calculate aggregates from readings (currently lines 32-56)
fn calculate_monthly_aggregates(readings: &[Reading]) -> MonthlyAggregates {
    // total_rainfall
    // reading_count
    // first_reading_date
    // last_reading_date
    // min_cumulative
    // max_cumulative
}

// Calculate month date range (currently lines 158-175)
fn calculate_month_range(year: i32, month: i32) -> (DateTime<Utc>, DateTime<Utc>) {
    // start_dt
    // end_dt
}
```

**2.2 Create domain types**

Location: `src/services/monthly_rainfall_service.rs` or `src/domain/` (if we create domain module)

```rust
pub struct MonthlyAggregates {
    pub total_rainfall: f64,
    pub reading_count: i32,
    pub first_reading_date: Option<DateTime<Utc>>,
    pub last_reading_date: Option<DateTime<Utc>>,
    pub min_cumulative: f64,
    pub max_cumulative: f64,
}
```

### Phase 3: Simplify Repository

**3.1 Update MonthlyRainfallRepository::upsert_monthly_summary**

Before (lines 20-93):
```rust
pub async fn upsert_monthly_summary(
    &self,
    station_id: &str,
    year: i32,
    month: i32,
    readings: &[Reading], // ❌ Performs calculations on readings
) -> Result<(), DbError>
```

After:
```rust
pub async fn upsert_monthly_summary(
    &self,
    station_id: &str,
    year: i32,
    month: i32,
    aggregates: &MonthlyAggregates, // ✅ Receives pre-calculated data
) -> Result<(), DbError>
```

**3.2 Update MonthlyRainfallRepository::recalculate_monthly_summary**

This method should be removed from repository and moved entirely to service layer:
- Service fetches readings via ReadingRepository
- Service calculates aggregates
- Service calls simplified upsert_monthly_summary

### Phase 4: Update Callers

**4.1 Identify all callers of MonthlyRainfallRepository**

Search for usage:
```bash
rg "MonthlyRainfallRepository" --type rust
```

Expected locations:
- API handlers (if any direct calls)
- Other services (ReadingService?)
- Schedulers (if updating summaries on scrape)

**4.2 Update callers to use MonthlyRainfallService**

Replace direct repository calls with service calls:

Before:
```rust
monthly_rainfall_repo.upsert_monthly_summary(
    station_id, year, month, &readings
).await?;
```

After:
```rust
monthly_rainfall_service.upsert_monthly_summary(
    station_id, year, month, &readings
).await?;
```

Service internally:
1. Calculates aggregates from readings
2. Calls repository with pre-calculated aggregates

### Phase 5: Dependency Injection

**5.1 Update AppState**

Location: `src/api.rs` or wherever AppState is defined

Before:
```rust
pub struct AppState {
    pub reading_repo: ReadingRepository,
    pub gauge_repo: GaugeRepository,
    pub monthly_rainfall_repo: MonthlyRainfallRepository,
    // ...
}
```

After:
```rust
pub struct AppState {
    pub reading_service: ReadingService,
    pub gauge_service: GaugeService,
    pub monthly_rainfall_service: MonthlyRainfallService, // ✅ Service not repo
    // ...
}
```

**5.2 Initialize service in main.rs**

```rust
let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
let reading_repo = ReadingRepository::new(pool.clone());

let monthly_rainfall_service = MonthlyRainfallService::new(
    monthly_rainfall_repo,
    reading_repo, // Needed for recalculation
);
```

### Phase 6: Testing

**6.1 Unit tests for service calculations**

Test without database:
```rust
#[test]
fn test_calculate_monthly_aggregates() {
    let readings = vec![
        Reading { incremental_inches: 0.5, cumulative_inches: 0.5, ... },
        Reading { incremental_inches: 1.0, cumulative_inches: 1.5, ... },
    ];

    let aggregates = MonthlyRainfallService::calculate_monthly_aggregates(&readings);

    assert_eq!(aggregates.total_rainfall, 1.5);
    assert_eq!(aggregates.reading_count, 2);
    // ... more assertions
}

#[test]
fn test_calculate_month_range() {
    let (start, end) = MonthlyRainfallService::calculate_month_range(2025, 3);

    assert_eq!(start.month(), 3);
    assert_eq!(end.month(), 4);
    // ... more assertions
}

#[test]
fn test_calculate_month_range_december() {
    let (start, end) = MonthlyRainfallService::calculate_month_range(2025, 12);

    assert_eq!(start.month(), 12);
    assert_eq!(start.year(), 2025);
    assert_eq!(end.month(), 1);
    assert_eq!(end.year(), 2026); // ✅ Year rollover
}
```

**6.2 Integration tests for repository**

Test database operations only (no business logic):
```rust
#[sqlx::test]
async fn test_upsert_monthly_summary(pool: PgPool) {
    let repo = MonthlyRainfallRepository::new(pool);
    let aggregates = MonthlyAggregates {
        total_rainfall: 2.5,
        reading_count: 10,
        // ...
    };

    repo.upsert_monthly_summary("TEST-001", 2025, 3, &aggregates)
        .await
        .unwrap();

    // Verify data was inserted
}
```

**6.3 Integration tests for service**

Test end-to-end with real database:
```rust
#[sqlx::test]
async fn test_service_recalculate_monthly_summary(pool: PgPool) {
    // Insert test readings
    // Call service.recalculate_monthly_summary()
    // Verify summary was correctly calculated and stored
}
```

## Implementation Order

1. ✅ **Create service file and module declaration** (Phase 1)
2. ✅ **Extract calculation methods** (Phase 2.1)
3. ✅ **Create domain types** (Phase 2.2)
4. ✅ **Simplify repository upsert method** (Phase 3.1)
5. ✅ **Move recalculate logic to service** (Phase 3.2)
6. ✅ **Update AppState** (Phase 5.1)
7. ✅ **Initialize service in main.rs** (Phase 5.2)
8. ✅ **Update all callers** (Phase 4)
9. ✅ **Add unit tests** (Phase 6.1)
10. ✅ **Add integration tests** (Phase 6.2-6.3)
11. ✅ **Update SQLx metadata** (`cargo sqlx prepare`)
12. ✅ **Run full CI checks** (`make ci-check`)

## Benefits

### Code Quality
- **Separation of concerns**: Business logic separated from data access
- **Single responsibility**: Repository only handles database, service handles business rules
- **Testability**: Can unit test calculations without database

### Maintainability
- **Easier to modify**: Change calculation logic without touching SQL
- **Easier to understand**: Clear layer boundaries
- **Easier to debug**: Isolate issues to specific layer

### Extensibility
- **Reusable logic**: Calculations can be used by multiple features (API, schedulers, imports)
- **Composition**: Service can orchestrate multiple repositories
- **Future features**: Can add caching, validation, notifications at service layer

## Migration Strategy

### Option A: Big Bang (Not Recommended)
- Refactor everything at once
- High risk of breaking changes
- Difficult to review

### Option B: Incremental (Recommended)
1. Create service alongside repository (both coexist)
2. Update callers one at a time to use service
3. Once all callers migrated, simplify repository
4. Less risky, easier to review, can be done across multiple PRs

## Rollback Plan

If issues arise:
1. Service layer can be removed (it's additive, not replacing)
2. Callers can revert to direct repository usage
3. Repository still has original implementation until Phase 3

## Future Considerations

### Domain Module (Optional)
If we accumulate more domain types, consider:
```
src/
├── domain/
│   ├── monthly_aggregates.rs
│   ├── water_year.rs (extract water year logic)
│   └── ...
```

### Caching Layer
Service layer is ideal place to add caching:
```rust
impl MonthlyRainfallService {
    async fn get_water_year_summaries(&self, ...) -> Result<...> {
        // Check cache
        if let Some(cached) = self.cache.get(...) {
            return Ok(cached);
        }

        // Fetch from repository
        let summaries = self.repo.get_water_year_summaries(...).await?;

        // Cache result
        self.cache.set(..., summaries.clone());

        Ok(summaries)
    }
}
```

### Event Publishing
Service can publish events for monitoring/audit:
```rust
self.event_bus.publish(MonthlyRainfallUpdated {
    station_id,
    year,
    month,
    total_rainfall,
}).await;
```

## Success Criteria

- [ ] All business logic moved to service layer
- [ ] Repository only contains SQL queries
- [ ] Unit tests added for calculation logic
- [ ] Integration tests pass
- [ ] CI pipeline passes (`make ci-check`)
- [ ] No breaking API changes (unless part of broader refactor)
- [ ] Performance unchanged or improved
- [ ] Code coverage maintained or improved

## Questions to Resolve

1. **Should we create a domain module?** Or keep domain types in service file?
2. **Should service own reading fetching?** Or accept readings as parameter?
3. **Should we add caching now?** Or leave for future iteration?
4. **How to handle transactions?** If service coordinates multiple repos, who manages transaction?

## References

- Similar pattern: `ReadingService` (src/services/reading_service.rs)
- Similar pattern: `GaugeService` (src/services/gauge_service.rs)
- CLAUDE.md architecture documentation
- Clean Architecture principles (Robert C. Martin)

## Related Work

This refactor aligns with potential future work:
- **Historical data import** (plans/historical-data-import.md): Service layer will make bulk recalculation easier
- **MCP server** (plans/mcp-server-implementation.md): Service provides clean API for external access
- **Nearby gauges feature** (plans/nearby-gauges-feature.md): May need to aggregate across multiple gauges

## Estimated Effort

- **Development**: 4-6 hours
- **Testing**: 2-3 hours
- **Review**: 1-2 hours
- **Total**: ~1 day

## Risk Assessment

**Low Risk**: This is primarily an internal refactor with minimal API surface changes. The incremental migration strategy further reduces risk.

**Mitigation**:
- Comprehensive test coverage before refactor
- Incremental migration (service alongside repository)
- Feature flag (optional): Could add feature flag to toggle between old/new implementation

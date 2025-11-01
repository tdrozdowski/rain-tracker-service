# Integration Test Architecture Refactor Plan

## Current State Analysis

### Problems with Current Architecture

#### 1. Shared Mutable Database State
- All tests share a single `rain_tracker_test` database
- `setup_test_db()` truncates ALL tables, affecting concurrent tests
- Tests must run serially (`#[serial]` attribute) to avoid data corruption
- Serial execution makes test suite slower as it grows

#### 2. Manual Cleanup is Error-Prone
- Each test must remember to call `cleanup_test_data()`
- If test panics before cleanup, data persists
- Cleanup code must be maintained separately from test logic
- No guarantee cleanup actually runs (panic, early return, etc.)

#### 3. Test Performance Impact
- Serial execution prevents parallel test runs
- Current: ~2.5 seconds for 69 tests (could be faster with parallelism)
- As test suite grows, serial execution becomes a bottleneck
- Database setup overhead repeated for each test

#### 4. Poor Isolation
- Tests can accidentally see data from other tests
- Hard to debug failures (is it my test or leftover data?)
- Foreign key constraints complicate cleanup order
- Race conditions when `#[serial]` is accidentally omitted

### Current Test Files Affected
- `tests/integration_test.rs` - 5 tests (all using `#[serial]`)
- `tests/api_integration_test.rs` - 11 tests (all using `#[serial]`)
- `tests/fopr_import_worker_test.rs` - 8 tests (all using `#[serial]`)
- Total: **24 integration tests** requiring serial execution

## Recommended Solution: Transaction-Based Testing

### Why Transaction-Based Testing?

**Advantages:**
- ✅ Perfect isolation: Each test gets a clean transaction
- ✅ Automatic rollback: No manual cleanup needed
- ✅ Fast: Rollback is instant, no table truncation
- ✅ Parallel execution: Tests don't interfere with each other
- ✅ No data leakage: Rollback guarantees clean state
- ✅ Better debugging: Test failures can't be caused by other tests

**Trade-offs:**
- ⚠️ Requires refactoring repositories to accept transactions
- ⚠️ Some SQLx queries may need adjustment
- ⚠️ Migrations still need to run once per test process

## Implementation Plan

### Phase 1: Infrastructure (Estimated: 4-6 hours)

#### 1.1 Create Test Transaction Helper
**File:** `tests/common/mod.rs` (new)

```rust
use sqlx::{PgPool, Postgres, Transaction};

/// Get a shared connection pool for all tests
/// Pool is created once and reused across tests
pub async fn test_pool() -> &'static PgPool {
    static INIT: std::sync::Once = std::sync::Once::new();
    static mut POOL: Option<PgPool> = None;

    unsafe {
        INIT.call_once(|| {
            let pool = tokio::runtime::Runtime::new()
                .unwrap()
                .block_on(async {
                    let database_url = std::env::var("DATABASE_URL")
                        .unwrap_or_else(|_| {
                            "postgres://postgres:password@localhost:5432/rain_tracker_test"
                                .to_string()
                        });

                    let pool = PgPoolOptions::new()
                        .max_connections(10) // Increase for parallel tests
                        .connect(&database_url)
                        .await
                        .expect("Failed to connect to test database");

                    // Run migrations once
                    sqlx::migrate!("./migrations")
                        .run(&pool)
                        .await
                        .expect("Failed to run migrations");

                    pool
                });
            POOL = Some(pool);
        });
        POOL.as_ref().unwrap()
    }
}

/// Begin a test transaction that will automatically rollback
pub async fn test_transaction() -> Transaction<'static, Postgres> {
    test_pool()
        .await
        .begin()
        .await
        .expect("Failed to begin transaction")
}
```

**Why this design:**
- Pool is created once and shared (fast)
- Migrations run once per test process
- Each test gets its own transaction
- Rollback on drop ensures cleanup

#### 1.2 Add Transaction Support to Repositories
**Files to modify:**
- `src/db/reading_repository.rs`
- `src/db/gauge_repository.rs`
- `src/db/monthly_rainfall_repository.rs`
- `src/db/fopr_import_job_repository.rs`

**Pattern to add:**
```rust
// Current: All methods use self.pool
pub async fn insert_reading(&self, reading: &RainReading, station_id: &str) -> Result<()> {
    sqlx::query!(/* ... */)
        .execute(&self.pool)  // Uses pool
        .await?;
    Ok(())
}

// Add: Transaction-aware variant
pub async fn insert_reading_tx(
    &self,
    tx: &mut Transaction<'_, Postgres>,
    reading: &RainReading,
    station_id: &str,
) -> Result<()> {
    sqlx::query!(/* ... */)
        .execute(&mut **tx)  // Uses transaction
        .await?;
    Ok(())
}
```

**Implementation strategy:**
1. Keep existing pool-based methods for production code
2. Add `_tx` variants for test isolation
3. Both methods share the same SQL query string (DRY)
4. Gradually migrate tests to use `_tx` methods

**Estimated effort:** 2-3 hours (4 repositories × ~30 min each)

### Phase 2: Refactor Integration Tests (Estimated: 3-4 hours)

#### 2.1 Update `tests/integration_test.rs`

**Before:**
```rust
#[tokio::test]
#[serial]  // ❌ Required due to shared state
async fn test_water_year_queries() {
    let pool = test_fixtures::setup_test_db().await;
    test_fixtures::cleanup_test_data(&pool, "TEST_WATER_YEAR_001").await;

    let reading_repo = ReadingRepository::new(pool.clone());
    // ... insert test data using pool

    // ... assertions

    // Manual cleanup (often forgotten)
    test_fixtures::cleanup_test_data(&pool, "TEST_WATER_YEAR_001").await;
}
```

**After:**
```rust
#[tokio::test]  // ✅ No #[serial] needed!
async fn test_water_year_queries() {
    use crate::common::test_transaction;

    let mut tx = test_transaction().await;

    // Repositories automatically use transaction
    let reading_repo = ReadingRepository::new_for_test();

    // ... insert test data using tx
    reading_repo.insert_reading_tx(&mut tx, &reading, "TEST_001").await.unwrap();

    // ... assertions (queries also use tx)

    // No cleanup needed - rollback is automatic!
}
```

**Steps:**
1. Remove `#[serial]` attributes
2. Replace `setup_test_db()` with `test_transaction()`
3. Update all repository calls to use `_tx` methods
4. Remove all `cleanup_test_data()` calls
5. Remove unique station_id generation (not needed anymore)

**Estimated effort:** 1.5 hours (5 tests × ~15 min each)

#### 2.2 Update `tests/api_integration_test.rs`

**Challenge:** Axum handlers need access to transaction, not pool.

**Solution:** Create test-specific service layer that uses transactions:

```rust
// Test helper
async fn create_test_app_with_tx(
    tx: &mut Transaction<'_, Postgres>,
) -> axum::Router {
    let reading_repo = ReadingRepository::new_for_test();
    let gauge_repo = GaugeRepository::new_for_test();
    let monthly_repo = MonthlyRainfallRepository::new_for_test();
    let job_repo = FoprImportJobRepository::new_for_test();

    // Services hold a reference to the transaction
    let reading_service = ReadingService::new_with_tx(
        reading_repo,
        monthly_repo,
        tx,
    );
    let gauge_service = GaugeService::new_with_tx(
        gauge_repo,
        job_repo,
        tx,
    );

    let state = AppState {
        reading_service,
        gauge_service,
    };

    create_router(state)
}
```

**Alternative (Simpler):** Keep API tests using pool-based repos, but add transaction wrapper:

```rust
#[tokio::test]
async fn test_water_year_endpoint() {
    let mut tx = test_transaction().await;

    // Insert test data using transaction
    insert_test_gauge_tx(&mut tx, "TEST_001").await;
    insert_test_readings_tx(&mut tx, "TEST_001", &readings).await;

    // Create app with pool (works because transaction sees its own writes)
    let pool = test_pool().await;
    let app = create_test_app(pool).await;

    // Make HTTP request (queries go through pool, but see uncommitted tx data)
    let response = app.oneshot(request).await.unwrap();

    // Assertions...

    // Rollback happens automatically
}
```

**Note:** This requires READ COMMITTED isolation level (Postgres default).

**Estimated effort:** 1.5-2 hours (11 tests × ~10 min each)

#### 2.3 Update `tests/fopr_import_worker_test.rs`

Similar pattern to integration tests:

```rust
#[tokio::test]  // ✅ No #[serial] needed!
async fn test_worker_claims_pending_job() {
    let mut tx = test_transaction().await;

    // Insert test gauge
    insert_test_gauge_tx(&mut tx, "TEST_001").await;

    let job_repo = FoprImportJobRepository::new_for_test();
    let job_id = job_repo.create_job_tx(&mut tx, "TEST_001", "test", 10, None).await.unwrap();

    // Claim job (uses same transaction)
    let claimed = job_repo.claim_next_job_tx(&mut tx).await.unwrap();

    assert!(claimed.is_some());
    assert_eq!(claimed.unwrap().id, job_id);

    // Automatic rollback - no cleanup needed
}
```

**Estimated effort:** 1 hour (8 tests × ~7 min each)

### Phase 3: Remove Serial Test Infrastructure (Estimated: 30 min)

#### 3.1 Remove `serial_test` Dependency
**File:** `Cargo.toml`

```toml
[dev-dependencies]
- serial_test = "3.2.0"  # Remove this line
```

#### 3.2 Clean Up Test Fixtures
**Files to modify:**
- `tests/integration_test.rs` - Remove `cleanup_all_test_data()`
- `tests/api_integration_test.rs` - Remove `cleanup_test_data()`
- `tests/fopr_import_worker_test.rs` - Remove `cleanup_test_data()`

#### 3.3 Update Documentation
**File:** `CLAUDE.md`

Update the testing section to explain transaction-based testing:

```markdown
### Testing with Transactions

All integration tests use transaction-based isolation:

```rust
#[tokio::test]
async fn test_something() {
    let mut tx = test_transaction().await;

    // Insert test data using _tx methods
    repo.insert_tx(&mut tx, data).await.unwrap();

    // Run assertions
    assert_eq!(result, expected);

    // Transaction rolls back automatically - no cleanup needed!
}
```

**Benefits:**
- No `#[serial]` needed - tests run in parallel
- No manual cleanup - rollback is automatic
- Perfect isolation - tests can't interfere
- Fast - rollback is instant
```

### Phase 4: Validation and Performance Testing (Estimated: 1 hour)

#### 4.1 Run Full Test Suite
```bash
DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker_test" \
  cargo test --all-targets
```

**Expected results:**
- All 69 tests pass
- Tests complete faster (parallel execution)
- No `#[serial]` attributes needed

#### 4.2 Measure Performance Improvement
```bash
# Before (serial)
time cargo test --all-targets  # Expected: ~2.5s

# After (parallel)
time cargo test --all-targets  # Expected: ~1.0-1.5s
```

#### 4.3 Verify Isolation
Run tests multiple times to ensure no race conditions:

```bash
for i in {1..10}; do
  echo "Run $i:"
  cargo test --all-targets || exit 1
done
```

All runs should pass consistently.

### Phase 5: CI/CD Updates (Estimated: 30 min)

#### 5.1 Update GitHub Actions Workflow
**File:** `.github/workflows/ci-cd.yml`

Ensure test jobs can run in parallel:

```yaml
- name: Run integration tests
  env:
    DATABASE_URL: ${{ env.DATABASE_URL_TEST }}
  run: cargo test --test '*' --verbose
  # No need to limit parallelism anymore!
```

#### 5.2 Update Coverage Job
**File:** `.github/workflows/ci-cd.yml`

```yaml
- name: Generate coverage report
  env:
    DATABASE_URL: ${{ env.DATABASE_URL_TEST }}
  run: |
    cargo llvm-cov --all-features --workspace --lcov --output-path lcov.info \
      --lib --bins \
      --test api_integration_test \
      --test integration_test \
      --test fopr_import_worker_test \
      --test fopr_metadata_parsing_test
```

## Alternative Approaches (Not Recommended)

### Alternative 1: Database-per-Test
**Approach:** Create and drop a database for each test.

**Pros:**
- Complete isolation
- No transaction complexity

**Cons:**
- Very slow (CREATE/DROP DATABASE is expensive)
- Requires superuser permissions
- Doesn't scale to hundreds of tests

**Verdict:** ❌ Too slow for CI/CD

### Alternative 2: In-Memory SQLite
**Approach:** Use SQLite `:memory:` for tests instead of Postgres.

**Pros:**
- Extremely fast
- Perfect isolation
- No external database needed

**Cons:**
- Tests against SQLite, not Postgres (SQL dialect differences)
- May miss Postgres-specific bugs
- Some SQLx features differ between databases

**Verdict:** ⚠️ Consider for unit tests, not integration tests

### Alternative 3: Keep Serial Execution
**Approach:** Leave everything as-is with `#[serial]`.

**Pros:**
- No refactoring needed
- Works today

**Cons:**
- Test suite gets slower as it grows
- Doesn't fix underlying architectural issues
- Technical debt accumulates

**Verdict:** ❌ Not sustainable long-term

## Rollback Plan

If transaction-based testing causes issues:

1. **Keep both patterns temporarily:**
   - New tests use transactions
   - Old tests keep `#[serial]` until refactored
   - Gradual migration

2. **Add feature flag for testing approach:**
   ```rust
   #[cfg(feature = "tx-tests")]
   pub async fn test_transaction() -> Transaction<'static, Postgres> { ... }

   #[cfg(not(feature = "tx-tests"))]
   pub async fn test_pool() -> PgPool { ... }
   ```

3. **Document any SQLx queries that don't work with transactions:**
   - Some DDL operations can't run in transactions
   - LISTEN/NOTIFY may behave differently
   - Keep list of exceptions

## Success Metrics

After completing this refactor:

- ✅ All 69 tests pass without `#[serial]`
- ✅ Test suite runs in <1.5 seconds (improvement from ~2.5s)
- ✅ No manual cleanup code in tests
- ✅ No data leakage between tests (verified by running 10+ times)
- ✅ CI/CD tests run faster
- ✅ Developer experience improved (tests are easier to write)

## Timeline Summary

| Phase | Tasks | Estimated Time |
|-------|-------|----------------|
| **Phase 1** | Infrastructure setup, repo refactoring | 4-6 hours |
| **Phase 2** | Refactor 24 integration tests | 4-5 hours |
| **Phase 3** | Remove serial test infrastructure | 30 min |
| **Phase 4** | Validation and performance testing | 1 hour |
| **Phase 5** | CI/CD updates | 30 min |
| **Total** | | **10-13 hours** |

## Getting Started

When ready to implement:

1. Create a new branch: `git checkout -b refactor/transaction-based-tests`
2. Start with Phase 1.1 (test transaction helper)
3. Pick one simple test to convert as proof-of-concept
4. Validate the approach before converting all tests
5. Submit PR with detailed testing notes

## Questions to Consider

Before starting implementation:

1. **SQLx version:** Are we on a version that fully supports transactions in tests?
2. **Nested transactions:** Do we need savepoints for any tests?
3. **Connection pooling:** Is 10 connections enough for parallel test execution?
4. **Read phenomena:** Do any tests require specific isolation levels?
5. **SQLx offline mode:** Will `_tx` methods work with SQLx prepare?

## References

- [SQLx Testing FAQ](https://github.com/launchbadge/sqlx/blob/main/FAQ.md#how-can-i-do-transactional-tests)
- [Postgres Transaction Isolation](https://www.postgresql.org/docs/current/transaction-iso.html)
- [Rust async testing patterns](https://rust-lang.github.io/async-book/08_ecosystem/00_chapter.html)
- [actix-web transaction testing example](https://github.com/actix/actix-web/discussions/2778)

---

**Document Version:** 1.0
**Created:** 2025-10-30
**Author:** Claude Code
**Status:** Proposal - Not Yet Implemented

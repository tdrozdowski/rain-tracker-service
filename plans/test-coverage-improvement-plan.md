# Test Coverage Improvement Plan

**Date Created:** 2025-11-05
**Last Updated:** 2025-11-05
**Starting Coverage:** 40.89%
**Current Coverage:** 43.51%
**Target Coverage:** 80%

## Overview

This plan outlines the strategy to improve test coverage from 40.89% to 80% by focusing on the files with the lowest coverage as identified by analyzing `lcov.info`.

## Coverage Analysis Method

**CRITICAL:** Always use `lcov.info` and the `analyze-coverage.py` script to guide testing efforts.

### Generate Coverage Report

```bash
DATABASE_URL="postgres://postgres:password@localhost:5432/rain_tracker_test" \
  cargo llvm-cov --all-targets --lcov --output-path lcov.info
```

### Analyze Coverage with Script

```bash
# Show files with lowest coverage
python3 scripts/analyze-coverage.py

# Show coverage for specific file
python3 scripts/analyze-coverage.py --filter excel_importer

# Show uncovered line numbers for specific file
python3 scripts/analyze-coverage.py --filter excel_importer --uncovered
```

**Script location:** `scripts/analyze-coverage.py`

**Features:**
- Parses lcov.info to extract line coverage data
- `--filter PATTERN`: Show only files matching pattern
- `--uncovered`: Display uncovered line numbers in ranges (e.g., "81-82, 91-94")
- Helps identify specific code paths needing tests

## Progress Summary

### Completed
- ✅ **excel_importer.rs**: 5.5% → 64.0% (+58.5%) - 21 tests added
  - Remaining uncovered: debug logs, error paths requiring malformed Excel data
  - Main business logic well covered
- ✅ **fopr_import_service.rs**: 17.4% → 20.7% (+3.3%) - 13 tests added
  - Partial coverage, needs integration tests for main business logic
- ✅ **Coverage analysis script**: Created `scripts/analyze-coverage.py`
  - Supports `--filter` and `--uncovered` flags
  - Reusable tool to avoid token waste

### Current Status (as of 2025-11-05)

**Overall Coverage:** 43.51% (started at 40.89%, gained 2.62%)

**Files Needing Work:**
- `src/db/fopr_import_job_repository.rs` - 37.2% (35/94) - Next target
- `src/importers/pdf_importer.rs` - 41.5% (97/234) - Large file
- `src/importers/downloader.rs` - 43.5% (37/85) - HTTP operations
- `src/importers/excel_importer.rs` - 64.0% (105/164) - Hit ceiling
- `src/workers/fopr_import_worker.rs` - 68.4% (13/19)
- `src/fopr/daily_data_parser.rs` - 74.5% (108/145)

**0% Coverage Files (Runtime/Startup - Lower Priority):**
- `src/app.rs`, `src/config.rs`, `src/db/pool.rs`, `src/main.rs`, `src/scheduler.rs`
- These are typically excluded from coverage targets (startup/runtime code)

## Original Plan

### High-Impact Files

#### 1. excel_importer.rs - ~~**5.5%**~~ **64.0% DONE** (105/164 lines)
- **Impact:** Massive - 164 lines, only 5.5% covered
- **Purpose:** Parses Excel files for historical rain data imports
- **Test Focus:**
  - Workbook opening and validation
  - Sheet reading (by name and index)
  - Cell value extraction (strings, numbers, dates)
  - Excel date serial number conversion
  - Error handling (missing files, corrupt workbooks, invalid cells)
  - Edge cases (empty cells, merged cells, different formats)

#### 2. fopr_import_service.rs - **17.4%** (16/92 lines) ⭐ HIGH PRIORITY
- **Impact:** High - 92 lines, core business logic
- **Purpose:** Orchestrates FOPR metadata and reading imports
- **Test Focus:**
  - Metadata parsing and validation
  - Reading import logic
  - Transaction handling
  - Foreign key violation handling
  - Error propagation and logging
  - Integration with gauge/reading repositories

#### 3. fopr_import_job_repository.rs - **37.2%** (35/94 lines)
- **Impact:** Medium-High - 94 lines, job queue critical
- **Purpose:** Manages FOPR import job queue
- **Test Focus:**
  - Job creation and enqueuing
  - Job claiming (worker assignment)
  - Status transitions (pending → running → completed/failed)
  - Retry logic and backoff calculation
  - Priority ordering
  - Error history tracking
  - Transaction methods

#### 4. pdf_importer.rs - **41.5%** (97/234 lines)
- **Impact:** Medium - 234 lines (largest file), already has some coverage
- **Purpose:** Parses PDF files for rainfall data
- **Test Focus:**
  - PDF file reading and validation
  - Text extraction from pages
  - Rainfall data parsing
  - Date parsing from PDF content
  - Error handling (corrupt PDFs, missing data)
  - Edge cases in data formats

#### 5. downloader.rs - **43.5%** (37/85 lines)
- **Impact:** Medium - 85 lines
- **Purpose:** Downloads files from URLs
- **Test Focus:**
  - HTTP GET requests
  - File writing to disk
  - Error handling (network errors, 404s, timeouts)
  - Content type validation
  - Directory creation
  - Cleanup on failure

## Testing Strategy

### Phase 1: Excel Importer ✅ COMPLETE
- ✅ Created `tests/excel_importer_test.rs` with 21 tests
- ✅ Tested workbook opening with valid files
- ✅ Tested error handling for missing/corrupt files
- ✅ Tested multiple gauges, dates, rainfall values
- ✅ Tested all months, water year logic
- ✅ **Result:** 5.5% → 64.0% (+58.5%)
- **Note:** Hit ceiling at 64% - remaining uncovered lines are debug logs and error paths requiring malformed Excel data

### Phase 2: FOPR Import Service ✅ PARTIAL
- ✅ Created `tests/fopr_import_service_test.rs` with 13 tests
- ✅ Tested error types (Display, Debug, From conversions)
- ✅ Tested service construction and cloning
- ✅ Tested `job_exists()` method
- ✅ Tested `month_date_range()` helper logic
- ⚠️ **Result:** 17.4% → 20.7% (+3.3%)
- **TODO:** Main `import_fopr()` business logic needs integration tests with real/mock downloads

### Phase 3: FOPR Import Job Repository (Target: 37.2% → 80%+) - NEXT
- [ ] Use script: `python3 scripts/analyze-coverage.py --filter fopr_import_job --uncovered`
- [ ] Create `tests/fopr_import_job_repository_test.rs`
- [ ] Test job creation with `create_job()`
- [ ] Test job claiming logic with `claim_next_job()`
- [ ] Test status transitions (pending → running → completed/failed)
- [ ] Test retry logic and backoff calculation
- [ ] Test priority ordering
- [ ] Test transaction methods (`create_job_tx`, etc.)
- [ ] **Run coverage:** `DATABASE_URL=... cargo llvm-cov --all-targets --lcov --output-path lcov.info`
- [ ] **Check improvement:** `python3 scripts/analyze-coverage.py --filter fopr_import_job`

### Phase 4: PDF Importer (Target: 41.5% → 80%+)
- [ ] Create `tests/pdf_importer_test.rs`
- [ ] Test PDF reading and parsing
- [ ] Test text extraction
- [ ] Test rainfall data parsing
- [ ] Test error handling
- [ ] **Run coverage and check lcov.info**

### Phase 5: Downloader (Target: 43.5% → 80%+)
- [ ] Create `tests/downloader_test.rs`
- [ ] Mock HTTP requests (use mockito or similar)
- [ ] Test successful downloads
- [ ] Test error scenarios
- [ ] Test file I/O
- [ ] **Run coverage and check lcov.info**

### Phase 6: Final Verification
- [ ] Run full coverage report
- [ ] Verify overall coverage ≥ 80%
- [ ] Review lcov.info for any remaining gaps
- [ ] Document any intentionally excluded files
- [ ] Commit all test improvements

## Files Already Well-Tested (No Action Needed)

- `src/api.rs` - 79.6% (API integration tests cover this)
- `src/fetcher.rs` - 87.3%
- `src/db/reading_repository.rs` - 87.9%
- `src/db/gauge_repository.rs` - Good coverage from recent work
- `src/db/monthly_rainfall_repository.rs` - Well tested
- `src/fopr/metadata_parser.rs` - 74.5% (has unit tests)
- `src/fopr/daily_data_parser.rs` - 74.5% (has unit tests)

## Important Reminders

1. **ALWAYS check lcov.info after adding tests** - Don't rely on the summary percentage alone
2. **Use the Python script above** to parse lcov.info and see which specific files improved
3. **Focus on business logic** - Runtime/startup code (main.rs, app.rs) is lower priority
4. **Write meaningful tests** - Don't just chase coverage numbers, test actual behavior
5. **Test error paths** - Many uncovered lines are error handling code
6. **Use test fixtures** - Create sample Excel/PDF files in `tests/fixtures/` for testing

## Test Data Requirements

### For Excel Importer Tests
- Create sample Excel files in `tests/fixtures/`:
  - Valid FOPR workbook with multiple sheets
  - Workbook with missing data
  - Corrupt/invalid workbook
  - Empty workbook

### For PDF Importer Tests
- Sample PDF files with rainfall data
- Corrupt PDF
- PDF with unexpected format

### For Downloader Tests
- Use `mockito` or similar HTTP mocking library
- Don't make real HTTP requests in tests

## Success Criteria

- [ ] Overall coverage ≥ 80%
- [ ] All high-impact files (excel_importer, fopr_import_service) ≥ 80%
- [ ] All tests pass in CI/CD
- [ ] Coverage report passing in CI/CD
- [ ] lcov.info shows consistent improvement across target files

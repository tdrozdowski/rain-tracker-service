# Uncovered Lines Analysis - Path to 80% Coverage

**Current:** 48.77% (1,759 covered / 3,607 total testable lines)
**Target:** 80% (2,886 lines needed)
**Gap:** 1,127 lines to cover

---

## High-Value Targets (>50 uncovered lines)

### 1. pdf_importer.rs - **137 uncovered lines** (41.5% → 80% = +90 lines, +2.5%)
**Uncovered:** 37-42, 45-46, 49-50, 53-60, 62-64, 69-70, 72, 75-80, 83-84, 86-87, 90-93, 95-97, 100-102, 104, 107-109, 112-114, 117-122, 124, 126-131, 134-141, 147, 150-151, 154, 157-158, 160-163, 166-168, 170-171, 187, 193-195, 203-210, 212-214, 217-218, 221-224, 226, 229-232, 235, 237, 239-247, 250-251, 259-261, 286, 290-291

**What's uncovered:**
- Main parsing logic (`parse_all_pages`, `parse_text`)
- Gauge group detection and parsing
- Date/rainfall extraction from text
- Error handling paths

**Required:** PDF sample files for testing

**Effort:** Medium-High (PDF parsing is complex)

---

### 2. fopr_import_service.rs - **73 uncovered lines** (20.7% → 80% = +54 lines, +1.5%)
**Uncovered:** 88, 114-115, 118, 120-121, 123-124, 127, 129-130, 132-133, 136, 138-139, 151-153, 156, 158-159, 172-173, 176, 178-179, 242-246, 261-263, 266, 268-269, 275, 287, 291-294, 307-309, 314, 316-317, 322, 327-331, 333-334, 336, 339-342, 344-345, 347-348, 353-356

**What's uncovered:**
- Main `import_fopr()` business logic
- Excel workbook opening and metadata parsing
- Bulk reading insertion
- Monthly summary recalculation
- Error handling for download/parse failures

**Required:** Integration tests with mocked HTTP downloads (already have mockito)

**Effort:** High (integration testing)

---

### 3. excel_importer.rs - **59 uncovered lines** (64.0% → 80% = +26 lines, +0.7%)
**Uncovered:** 81-82, 91-94, 125-126, 149, 151, 156-157, 160, 162, 167-168, 185-187, 191-195, 198, 200, 206, 220, 222-224, 231, 233-235, 237, 239-240, 243-247, 260, 271-278, 281-286

**What's uncovered:**
- Mostly logging (debug/warn statements)
- Error handling for malformed data
- Edge cases in date/value parsing

**Status:** Hit ceiling at 64% - remaining are hard to test without malformed Excel files

**Effort:** High for low value (need corrupt test data)

---

## Medium-Value Targets (10-50 uncovered lines)

### 4. daily_data_parser.rs - **37 uncovered lines** (74.5% → 90% = +22 lines, +0.6%)
**Uncovered:** 62, 85-86, 91, 95, 102, 106, 108, 129, 141-142, 148-150, 153, 156-157, 164-165, 168, 173, 178-179, 182, 189-190, 193, 200-201, 229, 266-268, 285, 296-298

**What's uncovered:**
- Error handling, logging
- Edge cases in FOPR daily data parsing

**Effort:** Medium

---

### 5. api.rs - **23 uncovered lines** (79.6% → 90% = +12 lines, +0.3%)
**Uncovered:** 168-170, 173-174, 210-212, 215-216, 249-251, 254-255, 295-298, 335-338

**What's uncovered:**
- Error response formatting
- Edge case handlers

**Effort:** Low-Medium (API endpoint tests)

---

### 6. fetcher.rs - **23 uncovered lines** (87.3% → 95% = +14 lines, +0.4%)
**Uncovered:** 30, 39, 57-58, 97, 106-108, 111, 115-116, 118, 125-126, 131-132, 264-265, 276

**What's uncovered:**
- Error handling, logging
- HTML scraping edge cases

**Effort:** Medium (needs HTML mock responses)

---

### 7. metadata_parser.rs - **18 uncovered lines** (94.8% → 98% = +11 lines, +0.3%)
**Uncovered:** 75-77, 85, 87-88, 96-98, 124, 142, 149, 160, 170, 238

**What's uncovered:**
- Error handling
- Edge cases in metadata parsing

**Effort:** Low-Medium

---

### 8. gauge_list_fetcher.rs - **15 uncovered lines** (93.4% → 98% = +11 lines, +0.3%)
**Uncovered:** 43, 52, 112-114, 120, 151, 163, 196, 203, 210

**What's uncovered:**
- Logging, error handling

**Effort:** Low

---

## Low-Value Targets (<10 uncovered lines)

### 9. reading_repository.rs - **7 uncovered** (87.9%)
Lines: 21, 57, 217-219, 221

### 10. gauge_service.rs - **7 uncovered** (89.6%)
Lines: 162-163, 166, 168-169, 191, 193

### 11. fopr_import_worker.rs - **4 uncovered** (68.4%)
Lines: 45, 65, 69, 174
(Note: These are infinite loop and function signatures - hard to test)

### 12. downloader.rs - **4 uncovered** (95.7%)
Lines: Already well-tested

### 13. gauge_repository.rs - **1 uncovered** (98.4%)
### 14. fopr_import_job_repository.rs - **1 uncovered** (98.9%)
### 15. reading_service.rs - **1 uncovered** (99.4%)

**Total low-value:** ~25 lines (+0.7%)

---

## Strategic Path to 80%

### Current Math:
- Total testable: 3,607 lines
- Currently covered: 1,759 (48.77%)
- Need for 80%: 2,886 lines
- **Gap: 1,127 lines**

### Realistic Scenarios:

#### Scenario A: Test Everything to 80%+ (Most Realistic)
1. pdf_importer.rs: 80% → **+90 lines (+2.5%)**
2. fopr_import_service.rs: 80% → **+54 lines (+1.5%)**
3. excel_importer.rs: 80% → **+26 lines (+0.7%)** *(hard due to ceiling)*
4. daily_data_parser.rs: 90% → **+22 lines (+0.6%)**
5. api.rs: 90% → **+12 lines (+0.3%)**
6. fetcher.rs: 95% → **+14 lines (+0.4%)**
7. metadata_parser.rs: 98% → **+11 lines (+0.3%)**
8. gauge_list_fetcher.rs: 98% → **+11 lines (+0.3%)**
9. All low-value to 100%: **+25 lines (+0.7%)**

**Total: ~265 lines = 55.1% total coverage** ❌ Still 24.9% short of 80%

---

#### Scenario B: Aggressive Push - Test EVERYTHING to Near-Perfection

Looking at the full inventory:
- pdf_importer: 137 lines → **+110 lines if 90%**
- fopr_import_service: 73 lines → **+60 lines if 90%**
- excel_importer: 59 lines → **+50 lines if 95%** (ceiling challenge)
- daily_data_parser: 37 lines → **+30 lines if 95%**
- api: 23 lines → **+20 lines if 95%**
- fetcher: 23 lines → **+20 lines if 95%**
- metadata_parser: 18 lines → **+15 lines if 98%**
- gauge_list_fetcher: 15 lines → **+12 lines if 98%**
- All others: **+35 lines to 100%**

**Total: ~352 lines = 58.5% total coverage** ❌ Still 21.5% short

---

## The Reality Check

**Even testing EVERY file to near-perfection only gets us to ~58-60% coverage.**

**Why?** Let's look at what we're covering:

Currently at 48.77% with:
- 3,607 total lines
- Excluded runtime (main, app, config, scheduler, pool): 190 lines

But wait... the TOTAL output shows:
```
TOTAL: 5607 lines, 1848 covered (48.77%)
```

This means there are **~2,000 lines in files we haven't even analyzed yet** or in bins/generated code.

### Missing from our analysis:
Let me check what files exist that we haven't covered...

---

## Next Steps

1. **Verify all source files are being measured** - check for missing modules
2. **Focus on the big 2:** pdf_importer + fopr_import_service (~144 lines = +4%)
3. **Improve all 70-90% files** to 95%+ (~100 lines = +2.8%)
4. **Investigate the gap** - why does `cargo llvm-cov` report 5,607 lines but we only see 3,607 in src/?

**Hypothesis:** We may have binary/integration test code being counted that shouldn't be, OR there are modules we haven't inventoried.

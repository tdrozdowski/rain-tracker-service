# Analysis of src/bin/ Directory

## Summary

**Total Files:** 7
**Total Lines:** ~2,141
**Recommendation:** Keep 2, Delete 5

---

## Files to KEEP (Production Utilities)

### 1. ✅ `generate-openapi.rs` (10 lines)
**Purpose:** Generates openapi.json from code annotations
**Status:** **ESSENTIAL** - Used in development workflow
**Used by:** `make openapi` (part of CI checks)
**Justification:** This is actively used in pre-commit hooks and CI to keep openapi.json in sync

**Keep:** ✅ YES

---

### 2. ✅ `historical_import.rs` (1,770 lines)
**Purpose:** Full-featured CLI tool for bulk historical data import
**Features:**
- Multiple import modes: Excel, PDF, FOPR (single/bulk)
- Downloads data from MCFCD website
- Gauge discovery from water year files
- Progress bars with indicatif
- Parallel downloads
- Database upserts with deduplication
- Monthly summary recalculation

**Status:** **PRODUCTION UTILITY** - This is a legitimate operational tool
**Justification:**
- Used for one-time bulk imports of historical data
- Would be needed if you add new gauges or need to backfill data
- Well-structured with clap CLI, proper error handling
- Not a debugging throwaway - this is a real maintenance tool

**Keep:** ✅ YES (but should NOT count toward service coverage)

---

## Files to DELETE (Temporary Debugging Tools)

### 3. ❌ `check_gauge.rs` (157 lines) - **DEBUGGING TOOL**
**Purpose:** Checks data for a specific gauge in database
**What it does:**
- Queries database for a gauge ID
- Shows data sources and counts
- Shows date ranges
- Shows latest/earliest readings

**Status:** TEMPORARY - Written to debug specific data issues
**Justification:** This is diagnostic code you wrote while debugging. The same info can be queried with SQL or through the API.

**Delete:** ❌ YES - Replace with SQL queries or use the service API

---

### 4. ❌ `check_gauges.rs` (48 lines) - **ONE-OFF TEST**
**Purpose:** Hardcoded test of gauges 62000 and 62200 from pcp1119.pdf
**What it does:**
- Parses a specific PDF (November 2019)
- Checks totals for two specific gauges
- Has expected values hardcoded: "expected: 3.78\", "expected: 0.00\""
- Mentions "Gauge 62200 was inoperative in November 2019"

**Status:** ONE-TIME DEBUGGING - Testing PDF parsing logic for specific case
**Justification:** This is clearly a throwaway validation test from when you were debugging PDF parsing. Not reusable.

**Delete:** ❌ YES - This should be a proper unit test, not a bin

---

### 5. ❌ `cleanup_pdf_data.rs` (21 lines) - **ONE-OFF CLEANUP**
**Purpose:** Deletes data from a specific PDF import (data_source = 'pdf_1119')
**What it does:**
- Hardcoded DELETE query for 'pdf_1119'
- No parameters, no flexibility

**Status:** ONE-TIME OPERATION - Cleaning up a specific import
**Justification:** This was a quick script to fix a data issue. If you need this again, just run SQL directly.

**Delete:** ❌ YES - Use SQL directly: `DELETE FROM rain_readings WHERE data_source = 'pdf_1119'`

---

### 6. ❌ `examine_fopr.rs` (66 lines) - **DEBUGGING TOOL**
**Purpose:** Inspects FOPR Excel file structure
**What it does:**
- Lists sheet names
- Prints first 40 rows of a sheet
- Shows cell contents for debugging

**Status:** TEMPORARY - Written to understand FOPR file format
**Justification:** This was exploration code while implementing FOPR parsing. Now that parsing works, this is dead weight.

**Delete:** ❌ YES - You already know the FOPR format. If needed, use Excel or a generic Excel viewer.

---

### 7. ❌ `list_gauges.rs` (69 lines) - **ONE-OFF UTILITY**
**Purpose:** Extracts gauge IDs from a water year Excel file
**What it does:**
- Reads OCT sheet from Excel file
- Extracts gauge IDs from row 3
- Writes to /tmp/gauge_ids.txt

**Status:** ONE-TIME OPERATION - Extracting gauge list for bulk import
**Justification:** This was probably used once to get a gauge list for `historical_import --gauge-list`. Now that you have the data, you don't need this.

**Delete:** ❌ YES - The gauge IDs are in your database now. If you need to extract them again, use the API or SQL.

---

## Recommended Actions

### 1. Delete the 5 debugging tools
```bash
git rm src/bin/check_gauge.rs
git rm src/bin/check_gauges.rs
git rm src/bin/cleanup_pdf_data.rs
git rm src/bin/examine_fopr.rs
git rm src/bin/list_gauges.rs
```

**Impact:**
- Removes 481 lines of dead code
- Reduces bin/ from 2,141 lines to 1,780 lines
- Keeps only production utilities

### 2. Keep the production utilities
- `generate-openapi.rs` - Essential for CI/CD
- `historical_import.rs` - Legitimate operational tool for bulk imports

### 3. Coverage impact
**Before cleanup:**
- Total bin lines: 2,141
- Dragging coverage down significantly

**After cleanup:**
- Total bin lines: 1,780
- Still shouldn't count toward service coverage (they're CLI tools)
- But much cleaner codebase

---

## Alternative: Create a `tools/` directory?

If you want to preserve the debugging tools for future reference without polluting `src/bin/`:

```bash
mkdir tools/debugging
git mv src/bin/check_gauge.rs tools/debugging/
git mv src/bin/check_gauges.rs tools/debugging/
git mv src/bin/cleanup_pdf_data.rs tools/debugging/
git mv src/bin/examine_fopr.rs tools/debugging/
git mv src/bin/list_gauges.rs tools/debugging/
```

This keeps them in the repo for reference but clearly marks them as non-production.

---

## My Recommendation

**Delete the 5 debugging tools.** Here's why:

1. **They're not needed** - The service is working, you have tests, and you have the real `historical_import` tool for actual operations

2. **They'll bitrot** - Code that's not maintained becomes a liability. If you ever need similar debugging, you'll write fresh code anyway.

3. **Git history preserves them** - If you really need one later, you can always `git checkout` an old commit

4. **Cleaner codebase** - Less clutter = easier to navigate

5. **Still won't fix coverage numbers** - Even without these 481 lines, the 1,780-line `historical_import.rs` is still a CLI tool that shouldn't count toward service coverage. The real fix was excluding `src/bin/` entirely.

---

## Summary Table

| File | Lines | Type | Keep? | Reason |
|------|-------|------|-------|--------|
| `generate-openapi.rs` | 10 | Production | ✅ YES | Used in CI/CD |
| `historical_import.rs` | 1,770 | Production | ✅ YES | Operational tool |
| `check_gauge.rs` | 157 | Debug | ❌ NO | Use SQL/API |
| `check_gauges.rs` | 48 | Debug | ❌ NO | One-off test |
| `cleanup_pdf_data.rs` | 21 | Debug | ❌ NO | One-off cleanup |
| `examine_fopr.rs` | 66 | Debug | ❌ NO | Exploration code |
| `list_gauges.rs` | 69 | Debug | ❌ NO | One-off extraction |

**Delete:** 361 lines of debugging code (5 files)
**Keep:** 1,780 lines of production utilities (2 files)

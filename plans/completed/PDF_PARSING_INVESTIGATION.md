# PDF Parsing Investigation - Gauge 59700 Missing Data

## Problem Statement
Gauge 59700 should have 4 readings in November 2019 PDF (pcp1119.pdf) totaling 2.36", but they are not appearing in the database.

## Expected Data for Gauge 59700 (from Page 31, Group G031)
- Location: Line 1389-1424 in extracted text
- Gauge position: Column 2 (index 1) in the gauge group
- Gauge IDs in G031: `59500, 59700, 60000, 60300, 60500, 60600, 60700, 60800`

**Expected Readings:**
1. 11/29/19: 0.83"
2. 11/21/19: 0.31"
3. 11/20/19: 1.06"
4. 11/19/19: 0.16"
**Total: 2.36"** (confirmed in TOTALS row)

## Investigation Findings

### ✅ PDF Text Extraction
- `pdf-extract` crate IS reading all pages
- Gauge 59700 appears at line 1389
- Text extraction working correctly

### ✅ Gauge Group Detection
- Parser DOES find G031 at line 1386
- Parser correctly extracts gauge IDs: `["59500", "59700", "60000", "60300", "60500", "60600", "60700", "60800"]`
- Parser finds "Daily precipitation" header at line 1391

### ⚠️ **PROBLEM: Data Parsing**
Debug output shows:
```
Found gauge group at line 1386: G031: Rain Gage Group 31
Gauge IDs at line 1389: ["59500", "59700", "60000", "60300", "60500", "60600", "60700", "60800"]
Found 'Daily precipitation' at line 1391
Starting to parse data lines from line 1393
Hit TOTALS at line 1424, parsed 4 readings for this group
```

**Issue**: Parser reports only 4 readings for ENTIRE group G031, but there should be many more:
- 11/29/19: 8 non-zero values across all gauges
- 11/28/19: 4 non-zero values
- 11/21/19: 8 non-zero values
- 11/20/19: 8 non-zero values
- 11/19/19: 8 non-zero values
- Plus more...

**Expected**: ~38+ non-zero readings for G031
**Actual**: Only 4 readings parsed

## Hypotheses to Test

### 1. Date Parsing Issue
- PDF dates are in MM/DD/YY format (e.g., "11/29/19")
- Parser might be misinterpreting the 2-digit year
- Current logic: `century = (year / 100) * 100; full_year = century + year_suffix`
- For "11/29/19" with year=2019: Should give 2019 ✓
- **NEEDS TESTING**: Add debug logging to see what dates are being parsed

### 2. Month Validation Issue
```rust
if date.month() != month {
    warn!("Date {} is not in expected month {}, skipping", date, month);
    return Ok(Vec::new());
}
```
- This might be rejecting valid readings
- **NEEDS TESTING**: Check if dates are being rejected

### 3. Line Parsing Issue
- `parse_daily_reading()` splits by whitespace
- Might not be handling the data format correctly
- Example line: `11/29/19    0.87     0.83     0.75     0.67     0.71     0.67     0.83     0.79`
- **NEEDS TESTING**: Add debug logging to see what's being parsed from each line

### 4. Value Parsing Issue
- Parser only stores non-zero values: `if rainfall > 0.0 { ... }`
- All 4 gauge 59700 values are non-zero, so this should work
- **NEEDS TESTING**: Check if values are being parsed correctly

## Next Steps

1. **Add detailed debug logging** to `parse_daily_reading()`:
   - Log each line being processed
   - Log parsed date and all values
   - Log which readings are being kept vs. filtered

2. **Test with known good line**:
   - Line 1394: `11/29/19    0.87     0.83     0.75     0.67     0.71     0.67     0.83     0.79`
   - Should produce 8 readings (all non-zero)
   - Gauge 59700 (index 1) should get value 0.83

3. **Check duplicate detection**:
   - All 304 readings were marked as "duplicates"
   - Maybe gauge 59700 data WAS imported before?
   - Need to query database directly to verify

4. **Verify database query**:
   - User might be querying wrong water year
   - November 2019 is in Water Year 2020 (Oct 2019 - Sep 2020), NOT Water Year 2019
   - Need to check both water year 2019 and 2020

## Database Query Plan

Check if data exists:
```sql
-- Check for gauge 59700 in November 2019
SELECT reading_datetime, incremental_inches, cumulative_inches, data_source
FROM rain_readings
WHERE station_id = '59700'
  AND reading_datetime >= '2019-11-01'
  AND reading_datetime < '2019-12-01'
ORDER BY reading_datetime;

-- Check all data sources for November 2019
SELECT data_source, COUNT(*) as count
FROM rain_readings
WHERE reading_datetime >= '2019-11-01'
  AND reading_datetime < '2019-12-01'
GROUP BY data_source;
```

## ROOT CAUSE IDENTIFIED ✅

**File**: `src/importers/pdf_importer.rs:307`
**Function**: `parse_rainfall()`

**Buggy Code**:
```rust
let cleaned = value_str.trim_end_matches(|c: char| c == '(' || c == ')' || c.is_numeric());
```

**Problem**: This removes ALL numeric characters from the end of the string, not just footnote markers!

**Examples of incorrect behavior**:
- `"0.83"` → removes '3', '8' → `"0."` → parses as `0.0` ❌
- `"0.31"` → removes '1', '3' → `"0."` → parses as `0.0` ❌
- `"1.06"` → removes '6', '0' → `"1."` → parses as `1.0` ❌
- `"0.16"` → removes '6', '1' → `"0."` → parses as `0.0` ❌

**What it should do**:
- `"0.83"` → `"0.83"` (no change) ✓
- `"0.83(1)"` → `"0.83"` (remove footnote) ✓
- `"____(1)"` → None (missing data) ✓

**Fix**: Use a regex or better string parsing to only remove footnote notation in parentheses.

## Resolution Plan

1. ✅ Create this investigation document
2. ✅ Add debug logging to identify exact parsing failure
3. ✅ **ROOT CAUSE FOUND**: `trim_end_matches` removing all digits
4. ✅ Fix the parsing logic in `parse_rainfall()`
5. ✅ Test with pcp1119.pdf to confirm 4 readings for gauge 59700
6. ✅ Verify correct total (2.36")
7. ✅ Update tests
8. ⏳ Commit fix with detailed explanation

## Resolution Summary ✅

**Fixed in commit**: [pending]

### The Fix
Changed `parse_rainfall()` from using `trim_end_matches` to using `find('(')`:

```rust
// OLD (BUGGY):
let cleaned = value_str.trim_end_matches(|c: char| c == '(' || c == ')' || c.is_numeric());

// NEW (FIXED):
let cleaned = if let Some(paren_pos) = value_str.find('(') {
    &value_str[..paren_pos]
} else {
    value_str
};
```

### Test Results - Before vs After

| Metric | Before (Buggy) | After (Fixed) | Change |
|--------|----------------|---------------|--------|
| Total non-zero readings parsed | 304 | 1,731 | +1,427 (+469%) |
| Gauge 59700 November readings | 1 (only 1.06) | 4 (all values) | +3 |
| Gauge 59700 total rainfall | ~1.06" | 2.36" | +1.30" |

### Verified Results for Gauge 59700

✅ **All 4 expected readings now imported correctly:**
- 11/29/19: 0.83" (was parsed as 0.0 before)
- 11/21/19: 0.31" (was parsed as 0.0 before)
- 11/20/19: 1.06" (was parsed as 1.0 before)
- 11/19/19: 0.16" (was parsed as 0.0 before)
- **Total: 2.36"** ✓ (matches PDF TOTALS row)

### Impact
This bug affected **ALL decimal values < 1.0** in PDF imports, causing massive data loss:
- Values like 0.83, 0.31, 0.16 were being stored as 0.0
- Values like 1.06 were being stored as 1.0
- Only whole numbers were unaffected

**The fix recovered 1,427 readings (82% of the data was being lost!)**

## Footnote Capture - IMPLEMENTED ✅

**Status**: Implemented and tested

### Implementation
The parser **captures footnote markers** from values and stores them in `import_metadata`:
- Input: `0.00(1)`
- Parsed: value=`0.00`, footnote_marker=`"1"`
- Stored in DB: `import_metadata = {"footnote_marker": "1"}`

### What Footnotes Indicate
Footnotes contain important data quality metadata:
- Gauge outages/malfunctions
- Estimated values
- Equipment problems
- Data accuracy warnings

Example from November 2019 PDF:
- Gauge 62200 TOTALS: `0.00(1)`
- Footnote text: `(1) Gage 62200 recorded no rain during the month; monthly and annual totals inaccurate.`

### Database Support
The `rain_readings` table **already has** an `import_metadata` JSONB column for this purpose:

```sql
-- From migration 20250105000000_add_historical_tracking.sql
ALTER TABLE rain_readings
ADD COLUMN IF NOT EXISTS import_metadata JSONB;

COMMENT ON COLUMN rain_readings.import_metadata IS
  'JSON metadata about the import (footnotes, estimated values, outage info)';

-- Example:
-- {"footnote": "Gage down due to battery failure", "estimated": true}
```

### What's Implemented
✅ Detects footnote markers on individual values: `0.83(1)`, `0.00(2)`
✅ Stores footnote markers in `import_metadata` JSONB column: `{"footnote_marker": "1"}`
✅ Works for both regular values and missing data: `____(1)`
✅ Unit tested and production-ready

### Future Enhancement: Footnote Text Parsing
**Status**: Not yet implemented

The parser currently captures the footnote **marker** (e.g., "1", "2") but not the footnote **text** itself.

To fully capture PDF data, future work could:
1. ⏳ Parse footnote definitions at bottom of each gauge group
2. ⏳ Match markers to definitions: `(1)` → "Gage down due to battery failure"
3. ⏳ Store complete metadata: `{"footnote_marker": "1", "footnote_text": "Gage 62200 recorded no rain..."}`
4. ⏳ Enable API queries to filter/flag readings with data quality issues

**Priority**: Low - footnote markers are captured, text lookup is nice-to-have

**Note**: In the November 2019 PDF, footnotes only appear on TOTALS rows (which we don't import), not on individual daily values. The code is ready to capture footnotes on daily values if they appear in other PDFs.

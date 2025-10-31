# FOPR Meta_Stats Sheet Parsing Specification

## Overview

This document specifies the exact parsing strategy for extracting gauge metadata from the `Meta_Stats` sheet in FOPR (Full Operational Period of Record) Excel files provided by the Maricopa County Flood Control District (MCFCD).

**Source Files Analyzed:**
- `sample-data-files/59700_FOPR.xlsx` (Aztec Park gauge)
- `sample-data-files/11000_FOPR.xlsx` (10th St. Wash Basin #1 gauge)

**Sheet Structure:**
- **Name**: `Meta_Stats` (always the first sheet, index 0)
- **Dimensions**: 38 rows × 17 columns (consistent across both samples)
- **Format**: Label-value pairs with labels in column A, values in column B (or C/D for some fields)

## Cell-to-Database Column Mapping

### Core Identification Fields

| Excel Location | Field Label | Sample Value | Database Column | Data Type | Notes |
|----------------|-------------|--------------|-----------------|-----------|-------|
| **B3** | Station Name | "Aztec Park" | `station_name` | VARCHAR(255) | Required |
| **B4** | Gage ID # History | "59700; 4695 prior to 2/20/2018" | `station_id` (current)<br>`previous_station_ids` (array) | VARCHAR(20)<br>TEXT[] | Parse semicolon-delimited; extract current ID + previous IDs |
| **B6** | Station Type | "Rain" or "Rain / Stage" | `station_type` | VARCHAR(50) | Default: 'Rain' |

### Location Fields

| Excel Location | Field Label | Sample Value | Database Column | Data Type | Notes |
|----------------|-------------|--------------|-----------------|-----------|-------|
| **C11** | Latitude (decimal) | 33.61006 | `latitude` | DECIMAL(10,7) | **Use column C** (decimal), not B (DMS) |
| **C12** | Longitude (decimal) | -111.86545 | `longitude` | DECIMAL(10,7) | **Use column C** (decimal), not B (DMS) |
| **B13** | Elevation | "1,465 ft." | `elevation_ft` | INTEGER | Strip comma and " ft." suffix |
| **B9** | City/Town | "Scottsdale" | `city` | VARCHAR(100) | Optional |
| **B10** | County | "Maricopa" | `county` | VARCHAR(100) | Default: 'Maricopa' |
| **B14** | Location Description | "Near Thunderbird & Frank Lloyd Wright" | `location_description` | TEXT | Optional |

### Operational Metadata

| Excel Location | Field Label | Sample Value | Database Column | Data Type | Notes |
|----------------|-------------|--------------|-----------------|-----------|-------|
| **B8** | Data Begins | 35835 (Excel serial) | `data_begins_date` | DATE | Convert Excel date serial to DATE |
| **B7** | Years Since Installation | 26.642026009582477 | `installation_date` | DATE | Calculate: reference_date (D7) - (years * 365.25) |
| **D7** | Reference date for years | 45566 (Excel serial) | Used for calculation | DATE | Convert Excel serial, use to compute installation_date |

### Climate Statistics

| Excel Location | Field Label | Sample Value | Database Column | Data Type | Notes |
|----------------|-------------|--------------|-----------------|-----------|-------|
| **D15** | Avg Annual Precip | 7.4803 | `avg_annual_precipitation_inches` | DECIMAL(6,2) | Located in column D, not B |
| **A15** | Complete Years Count | "...for 26 Complete Years..." | `complete_years_count` | INTEGER | Parse integer from label in column A |

### Data Quality

| Excel Location | Field Label | Sample Value | Database Column | Data Type | Notes |
|----------------|-------------|--------------|-----------------|-----------|-------|
| **B16** | Incomplete Months | "None" | `incomplete_months_count` | INTEGER | "None" → 0; parse integer otherwise |
| **B17** | Missing Months | "None" | `missing_months_count` | INTEGER | "None" → 0; parse integer otherwise |
| **B18** | Remarks | "Records Good" | `data_quality_remarks` | TEXT | Optional |

### Frequency Statistics (Store in JSONB)

All frequency statistics go into the `fopr_metadata` JSONB column:

| Excel Location | Field Label | Sample Value | JSONB Key | Notes |
|----------------|-------------|--------------|-----------|-------|
| **C25** | Storms > 1" in 24h | 35 | `storms_gt_1in_24h` | Integer count |
| **C26** | Storms > 2" in 24h | 4 | `storms_gt_2in_24h` | Integer count |
| **C27** | Storms > 3" in 24h | 0 | `storms_gt_3in_24h` | Integer count |
| **B31** | Greatest 15-min | 0.91 | `freq_15min_inches` | Decimal (inches) |
| **C31** | 15-min date | 38566 (Excel serial) | `freq_15min_date` | Date string |
| **D31** | 15-min return period | 20 | `freq_15min_return_period_yrs` | Integer (years) |
| **B32** | Greatest 1-hour | 1.3 | `freq_1hr_inches` | Decimal (inches) |
| **C32** | 1-hour date | 44785 | `freq_1hr_date` | Date string |
| **D32** | 1-hour return period | 10 | `freq_1hr_return_period_yrs` | Integer (years) |
| **B33** | Greatest 3-hour | 1.5 | `freq_3hr_inches` | Decimal (inches) |
| **C33** | 3-hour date | 41909 | `freq_3hr_date` | Date string |
| **D33** | 3-hour return period | 9 | `freq_3hr_return_period_yrs` | Integer (years) |
| **B34** | Greatest 6-hour | 1.57 | `freq_6hr_inches` | Decimal (inches) |
| **C34** | 6-hour date | 41890 | `freq_6hr_date` | Date string |
| **D34** | 6-hour return period | 6 | `freq_6hr_return_period_yrs` | Integer (years) |
| **B35** | Greatest 24-hour | 2.64 | `freq_24hr_inches` | Decimal (inches) |
| **C35** | 24-hour date | 43375 | `freq_24hr_date` | Date string |
| **D35** | 24-hour return period | 14 | `freq_24hr_return_period_yrs` | Integer (years) |
| **B36** | Greatest 72-hour | 3.35 | `freq_72hr_inches` | Decimal (inches) |
| **C36** | 72-hour date | 44421 | `freq_72hr_date` | Date string |
| **D36** | 72-hour return period | 15 | `freq_72hr_return_period_yrs` | Integer (years) |

## Excel Date Serial Conversion

**Excel stores dates as integers** (serial numbers):
- **Epoch**: December 31, 1899 (serial 0)
- **Example**: 35835 = January 10, 1998
- **Example**: 45566 = October 1, 2024

**Conversion formula:**
```rust
use chrono::{NaiveDate, Duration};

const EXCEL_EPOCH: NaiveDate = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();

fn excel_serial_to_date(serial: f64) -> Option<NaiveDate> {
    EXCEL_EPOCH.checked_add_signed(Duration::days(serial as i64))
}
```

**Fields requiring conversion:**
- `data_begins_date` (B8)
- Reference date for installation calculation (D7)
- All frequency statistic dates (C31-C36)

## Installation Date Calculation

**Formula**: `installation_date = reference_date - (years_since_installation * 365.25)`

```rust
fn calculate_installation_date(years_since: f64, reference_serial: f64) -> Option<NaiveDate> {
    let reference_date = excel_serial_to_date(reference_serial)?;
    let days_offset = (years_since * 365.25) as i64;
    reference_date.checked_sub_signed(Duration::days(days_offset))
}
```

**Example (gauge 59700):**
- Years Since Installation: 26.642026009582477
- Reference Date: 45566 (Oct 1, 2024)
- Installation Date: Oct 1, 2024 - (26.64 * 365.25 days) ≈ Jan 20, 1998

## Complete Years Count Extraction

**Location**: Row 15, Column A (the label itself)

**Sample**: `"Average Annual Precipitation for 26 Complete Years (in):"`

**Extraction strategy**:
```rust
fn extract_complete_years(label: &str) -> Option<i32> {
    // Regex: r"for\s+(\d+)\s+Complete Years"
    let re = Regex::new(r"for\s+(\d+)\s+Complete Years").unwrap();
    re.captures(label)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
}
```

## Gage ID History Parsing

**Format**: `"59700; 4695 prior to 2/20/2018"`

**Strategy**:
```rust
struct GageIdHistory {
    current_id: String,      // "59700"
    previous_ids: Vec<String>, // ["4695"]
}

fn parse_gage_id_history(value: &str) -> GageIdHistory {
    let parts: Vec<&str> = value.split(';').map(|s| s.trim()).collect();

    let current_id = parts[0].to_string();

    // Extract previous IDs from subsequent parts
    let previous_ids = parts[1..]
        .iter()
        .filter_map(|part| {
            // Extract ID from "4695 prior to 2/20/2018" format
            part.split_whitespace().next().map(|s| s.to_string())
        })
        .collect();

    GageIdHistory { current_id, previous_ids }
}
```

## Edge Cases and Validation

### Missing or "None" Values

| Field | Excel Value | Database Value | Validation |
|-------|-------------|----------------|------------|
| Incomplete Months | "None" | 0 | If not "None", parse integer |
| Missing Months | "None" | 0 | If not "None", parse integer |
| Previous IDs | Only current ID listed | Empty array `[]` | Check for semicolon; if absent, no previous IDs |
| City | Empty cell | NULL | Optional field |
| Location Description | Empty cell | NULL | Optional field |

### Data Type Validation

```rust
// Latitude range check
fn validate_latitude(lat: f64) -> bool {
    lat >= 32.0 && lat <= 34.0  // Maricopa County bounds
}

// Longitude range check
fn validate_longitude(lon: f64) -> bool {
    lon >= -113.0 && lon <= -111.0  // Maricopa County bounds
}

// Elevation range check
fn validate_elevation(elev: i32) -> bool {
    elev >= 500 && elev <= 4000  // Reasonable range for Maricopa County
}

// Precipitation sanity check
fn validate_precipitation(inches: f64) -> bool {
    inches >= 0.0 && inches <= 20.0  // Maricopa annual avg is ~7-9 inches
}
```

### Format Variations

**Elevation formats seen:**
- `"1,465 ft."` (with comma)
- `"1320 ft."` (no comma)

**Parsing strategy**:
```rust
fn parse_elevation(value: &str) -> Option<i32> {
    value
        .replace(",", "")           // Remove commas
        .split_whitespace()         // Split on whitespace
        .next()                     // Take first token
        .and_then(|s| s.parse::<i32>().ok())
}
```

**Station Type variations:**
- `"Rain"` (most common)
- `"Rain / Stage"` (dual-purpose gauges)

**Parsing strategy**: Store as-is, normalize later if needed

## Parsing Data Structures

### Rust Structs

```rust
use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MetaStatsData {
    // Identification
    pub station_id: String,
    pub station_name: String,
    pub previous_station_ids: Vec<String>,
    pub station_type: String,

    // Location
    pub latitude: f64,
    pub longitude: f64,
    pub elevation_ft: Option<i32>,
    pub county: String,
    pub city: Option<String>,
    pub location_description: Option<String>,

    // Operational metadata
    pub installation_date: Option<NaiveDate>,
    pub data_begins_date: Option<NaiveDate>,
    pub status: String, // Default: "Active"

    // Climate statistics
    pub avg_annual_precipitation_inches: Option<f64>,
    pub complete_years_count: Option<i32>,

    // Data quality
    pub incomplete_months_count: i32,
    pub missing_months_count: i32,
    pub data_quality_remarks: Option<String>,

    // FOPR metadata (JSONB)
    pub fopr_metadata: serde_json::Map<String, JsonValue>,
}

impl MetaStatsData {
    /// Create from calamine Range
    pub fn from_worksheet_range(range: &Range<DataType>) -> Result<Self, ParseError> {
        // Implementation in next section
        todo!()
    }
}
```

### Parsing Implementation Outline

```rust
use calamine::{DataType, Range};

impl MetaStatsData {
    pub fn from_worksheet_range(range: &Range<DataType>) -> Result<Self, ParseError> {
        // Helper to get cell value safely (0-indexed)
        let get_cell = |row: usize, col: usize| -> Option<String> {
            range.get_value((row, col))
                .and_then(|v| match v {
                    DataType::String(s) => Some(s.clone()),
                    DataType::Float(f) => Some(f.to_string()),
                    DataType::Int(i) => Some(i.to_string()),
                    _ => None,
                })
        };

        // Parse Gage ID History (Row 4, Col B = index 3, 1)
        let gage_history_str = get_cell(3, 1)
            .ok_or(ParseError::MissingField("Gage ID History"))?;
        let gage_history = parse_gage_id_history(&gage_history_str);

        // Extract station name (Row 3, Col B)
        let station_name = get_cell(2, 1)
            .ok_or(ParseError::MissingField("Station Name"))?;

        // Extract station type (Row 6, Col B)
        let station_type = get_cell(5, 1).unwrap_or_else(|| "Rain".to_string());

        // Extract latitude (Row 11, Col C = index 10, 2)
        let latitude = get_cell(10, 2)
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or(ParseError::MissingField("Latitude"))?;

        // Extract longitude (Row 12, Col C = index 11, 2)
        let longitude = get_cell(11, 2)
            .and_then(|s| s.parse::<f64>().ok())
            .ok_or(ParseError::MissingField("Longitude"))?;

        // Extract elevation (Row 13, Col B)
        let elevation_ft = get_cell(12, 1).and_then(|s| parse_elevation(&s));

        // Extract city (Row 9, Col B)
        let city = get_cell(8, 1);

        // Extract county (Row 10, Col B)
        let county = get_cell(9, 1).unwrap_or_else(|| "Maricopa".to_string());

        // Extract location (Row 14, Col B)
        let location_description = get_cell(13, 1);

        // Parse dates
        let data_begins_date = get_cell(7, 1)
            .and_then(|s| s.parse::<f64>().ok())
            .and_then(excel_serial_to_date);

        let years_since = get_cell(6, 1).and_then(|s| s.parse::<f64>().ok());
        let reference_date_serial = get_cell(6, 3).and_then(|s| s.parse::<f64>().ok());
        let installation_date = match (years_since, reference_date_serial) {
            (Some(years), Some(ref_serial)) => {
                calculate_installation_date(years, ref_serial)
            }
            _ => None,
        };

        // Parse climate stats
        let avg_annual_precipitation_inches = get_cell(14, 3)
            .and_then(|s| s.parse::<f64>().ok());

        let complete_years_count = get_cell(14, 0) // Column A label
            .and_then(|s| extract_complete_years(&s));

        // Parse data quality
        let incomplete_months_count = get_cell(15, 1)
            .map(|s| if s.to_lowercase() == "none" { 0 } else { s.parse::<i32>().unwrap_or(0) })
            .unwrap_or(0);

        let missing_months_count = get_cell(16, 1)
            .map(|s| if s.to_lowercase() == "none" { 0 } else { s.parse::<i32>().unwrap_or(0) })
            .unwrap_or(0);

        let data_quality_remarks = get_cell(17, 1);

        // Build FOPR metadata JSONB
        let mut fopr_metadata = serde_json::Map::new();

        // Storm counts
        if let Some(val) = get_cell(24, 2).and_then(|s| s.parse::<i32>().ok()) {
            fopr_metadata.insert("storms_gt_1in_24h".to_string(), JsonValue::from(val));
        }
        if let Some(val) = get_cell(25, 2).and_then(|s| s.parse::<i32>().ok()) {
            fopr_metadata.insert("storms_gt_2in_24h".to_string(), JsonValue::from(val));
        }
        if let Some(val) = get_cell(26, 2).and_then(|s| s.parse::<i32>().ok()) {
            fopr_metadata.insert("storms_gt_3in_24h".to_string(), JsonValue::from(val));
        }

        // Frequency statistics (rows 31-36, 0-indexed: 30-35)
        // 15-min
        if let Some(val) = get_cell(30, 1).and_then(|s| s.parse::<f64>().ok()) {
            fopr_metadata.insert("freq_15min_inches".to_string(), JsonValue::from(val));
        }
        // ... (similar for other frequency stats)

        Ok(MetaStatsData {
            station_id: gage_history.current_id,
            station_name,
            previous_station_ids: gage_history.previous_ids,
            station_type,
            latitude,
            longitude,
            elevation_ft,
            county,
            city,
            location_description,
            installation_date,
            data_begins_date,
            status: "Active".to_string(),
            avg_annual_precipitation_inches,
            complete_years_count,
            incomplete_months_count,
            missing_months_count,
            data_quality_remarks,
            fopr_metadata,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid data format: {0}")]
    InvalidFormat(String),

    #[error("Validation failed: {0}")]
    ValidationError(String),
}
```

## Validation Rules Summary

**Required fields** (parsing will fail if missing):
- Station ID (B4)
- Station Name (B3)
- Latitude (C11)
- Longitude (C12)

**Optional fields** (NULL if missing):
- Elevation
- City
- Location Description
- Installation Date (requires both years_since and reference_date)
- Data Begins Date
- Average Annual Precipitation
- Complete Years Count
- Data Quality Remarks

**Defaults**:
- County: "Maricopa"
- Station Type: "Rain"
- Incomplete Months: 0
- Missing Months: 0
- Status: "Active"
- Previous Station IDs: Empty array `[]`

## Testing Strategy

### Unit Tests

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gage_id_history_with_previous() {
        let input = "59700; 4695 prior to 2/20/2018";
        let result = parse_gage_id_history(input);
        assert_eq!(result.current_id, "59700");
        assert_eq!(result.previous_ids, vec!["4695"]);
    }

    #[test]
    fn test_parse_gage_id_history_no_previous() {
        let input = "11000";
        let result = parse_gage_id_history(input);
        assert_eq!(result.current_id, "11000");
        assert!(result.previous_ids.is_empty());
    }

    #[test]
    fn test_excel_serial_to_date() {
        // 35835 = January 10, 1998
        let date = excel_serial_to_date(35835.0).unwrap();
        assert_eq!(date.year(), 1998);
        assert_eq!(date.month(), 1);
        assert_eq!(date.day(), 10);
    }

    #[test]
    fn test_parse_elevation_with_comma() {
        assert_eq!(parse_elevation("1,465 ft."), Some(1465));
    }

    #[test]
    fn test_parse_elevation_no_comma() {
        assert_eq!(parse_elevation("1320 ft."), Some(1320));
    }

    #[test]
    fn test_extract_complete_years() {
        let label = "Average Annual Precipitation for 26 Complete Years (in):";
        assert_eq!(extract_complete_years(label), Some(26));
    }

    #[test]
    fn test_validate_latitude_maricopa_range() {
        assert!(validate_latitude(33.61006));
        assert!(!validate_latitude(40.0)); // Too far north
    }

    #[test]
    fn test_validate_longitude_maricopa_range() {
        assert!(validate_longitude(-111.86545));
        assert!(!validate_longitude(-100.0)); // Too far east
    }
}
```

### Integration Tests

```rust
#[test]
fn test_parse_meta_stats_59700() {
    let mut workbook = open_workbook_auto("sample-data-files/59700_FOPR.xlsx").unwrap();
    let range = workbook.worksheet_range("Meta_Stats").unwrap();

    let meta = MetaStatsData::from_worksheet_range(&range).unwrap();

    assert_eq!(meta.station_id, "59700");
    assert_eq!(meta.station_name, "Aztec Park");
    assert_eq!(meta.previous_station_ids, vec!["4695"]);
    assert_eq!(meta.station_type, "Rain");
    assert_eq!(meta.latitude, 33.61006);
    assert_eq!(meta.longitude, -111.86545);
    assert_eq!(meta.elevation_ft, Some(1465));
    assert_eq!(meta.city, Some("Scottsdale".to_string()));
    assert_eq!(meta.county, "Maricopa");
    assert_eq!(meta.complete_years_count, Some(26));

    // Verify JSONB contains storm counts
    assert_eq!(meta.fopr_metadata.get("storms_gt_1in_24h"), Some(&JsonValue::from(35)));
}

#[test]
fn test_parse_meta_stats_11000() {
    let mut workbook = open_workbook_auto("sample-data-files/11000_FOPR.xlsx").unwrap();
    let range = workbook.worksheet_range("Meta_Stats").unwrap();

    let meta = MetaStatsData::from_worksheet_range(&range).unwrap();

    assert_eq!(meta.station_id, "11000");
    assert_eq!(meta.station_name, "10th St. Wash Basin # 1");
    assert_eq!(meta.station_type, "Rain / Stage");
    assert_eq!(meta.complete_years_count, Some(27));
}
```

## Next Steps

1. **Implement the parsing functions** in `src/fopr/metadata_parser.rs`
2. **Add calamine dependency** with date feature: `calamine = { version = "0.31", features = ["dates"] }`
3. **Write unit tests** for each helper function
4. **Write integration tests** using the sample FOPR files
5. **Create database insertion logic** to upsert metadata into `gauges` table
6. **Document error handling** for malformed FOPR files

## References

- **Excel date serial format**: [Microsoft Excel Date System](https://support.microsoft.com/en-us/office/date-systems-in-excel-e7fe7167-48a9-4b96-bb53-5612a800b487)
- **Calamine library**: [docs.rs/calamine](https://docs.rs/calamine)
- **Maricopa County Flood Control**: [MCFCD Rainfall Data](https://alert.fcd.maricopa.gov)

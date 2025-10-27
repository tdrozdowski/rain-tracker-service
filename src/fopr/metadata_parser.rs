/// FOPR Meta_Stats Sheet Metadata Parser
///
/// Parses gauge metadata from the Meta_Stats sheet in FOPR Excel files.
/// See docs/fopr-meta-stats-parsing-spec.md for detailed cell mapping specification.
use calamine::{Data, Range};
use chrono::{Duration, NaiveDate};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

/// Gauge metadata extracted from FOPR Meta_Stats sheet
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

/// Gage ID with historical ID tracking
#[derive(Debug, Clone)]
struct GageIdHistory {
    current_id: String,
    previous_ids: Vec<String>,
}

/// Parse errors
#[derive(Debug, thiserror::Error)]
pub enum ParseError {
    #[error("Missing required field: {0}")]
    MissingField(&'static str),

    #[error("Invalid data format: {0}")]
    InvalidFormat(String),

    #[error("Validation failed: {0}")]
    ValidationError(String),
}

impl MetaStatsData {
    /// Parse metadata from Meta_Stats worksheet range
    pub fn from_worksheet_range(range: &Range<Data>) -> Result<Self, ParseError> {
        // Helper to get cell value safely (0-indexed)
        let get_cell = |row: usize, col: usize| -> Option<String> {
            range.get((row, col)).and_then(|v| match v {
                Data::String(s) => Some(s.clone()),
                Data::Float(f) => Some(f.to_string()),
                Data::Int(i) => Some(i.to_string()),
                _ => None,
            })
        };

        // Helper to get numeric cell value
        let get_float = |row: usize, col: usize| -> Option<f64> {
            range.get((row, col)).and_then(|v| match v {
                Data::Float(f) => Some(*f),
                Data::Int(i) => Some(*i as f64),
                Data::DateTime(dt) => Some(dt.as_f64()),
                Data::String(s) => s.parse::<f64>().ok(),
                _ => None,
            })
        };

        // Helper to get date cell value (handles ExcelDateTime)
        let get_date = |row: usize, col: usize| -> Option<NaiveDate> {
            range.get((row, col)).and_then(|v| match v {
                Data::DateTime(dt) => excel_datetime_to_date(dt),
                Data::Float(f) => excel_serial_to_date(*f),
                Data::Int(i) => excel_serial_to_date(*i as f64),
                _ => None,
            })
        };

        // Parse Gage ID History (Row 4, Col B = index 3, 1)
        let gage_history_str = get_cell(3, 1).ok_or(ParseError::MissingField("Gage ID History"))?;
        let gage_history = parse_gage_id_history(&gage_history_str);

        // Extract station name (Row 3, Col B)
        let station_name = get_cell(2, 1).ok_or(ParseError::MissingField("Station Name"))?;

        // Extract station type (Row 6, Col B)
        let station_type = get_cell(5, 1).unwrap_or_else(|| "Rain".to_string());

        // Extract latitude (Row 11, Col C = index 10, 2)
        let latitude = get_float(10, 2).ok_or(ParseError::MissingField("Latitude"))?;
        validate_latitude(latitude)?;

        // Extract longitude (Row 12, Col C = index 11, 2)
        let longitude = get_float(11, 2).ok_or(ParseError::MissingField("Longitude"))?;
        validate_longitude(longitude)?;

        // Extract elevation (Row 13, Col B)
        let elevation_ft = get_cell(12, 1).and_then(|s| parse_elevation(&s));
        if let Some(elev) = elevation_ft {
            validate_elevation(elev)?;
        }

        // Extract city (Row 9, Col B)
        let city = get_cell(8, 1).filter(|s| !s.is_empty());

        // Extract county (Row 10, Col B)
        let county = get_cell(9, 1).unwrap_or_else(|| "Maricopa".to_string());

        // Extract location (Row 14, Col B)
        let location_description = get_cell(13, 1).filter(|s| !s.is_empty());

        // Parse dates
        let data_begins_date = get_date(7, 1);

        let years_since = get_float(6, 1);
        let reference_date_serial = get_float(6, 3);
        let installation_date = match (years_since, reference_date_serial) {
            (Some(years), Some(ref_serial)) => calculate_installation_date(years, ref_serial),
            _ => None,
        };

        // Parse climate stats
        let avg_annual_precipitation_inches = get_float(14, 3);
        if let Some(precip) = avg_annual_precipitation_inches {
            validate_precipitation(precip)?;
        }

        let complete_years_count = get_cell(14, 0) // Column A label
            .and_then(|s| extract_complete_years(&s));

        // Parse data quality
        let incomplete_months_count = get_cell(15, 1)
            .map(|s| {
                if s.to_lowercase() == "none" {
                    0
                } else {
                    s.parse::<i32>().unwrap_or(0)
                }
            })
            .unwrap_or(0);

        let missing_months_count = get_cell(16, 1)
            .map(|s| {
                if s.to_lowercase() == "none" {
                    0
                } else {
                    s.parse::<i32>().unwrap_or(0)
                }
            })
            .unwrap_or(0);

        let data_quality_remarks = get_cell(17, 1).filter(|s| !s.is_empty());

        // Build FOPR metadata JSONB
        let mut fopr_metadata = serde_json::Map::new();

        // Storm counts (rows 25-27, 0-indexed: 24-26, col C = index 2)
        if let Some(val) = get_float(24, 2).map(|f| f as i32) {
            fopr_metadata.insert("storms_gt_1in_24h".to_string(), JsonValue::from(val));
        }
        if let Some(val) = get_float(25, 2).map(|f| f as i32) {
            fopr_metadata.insert("storms_gt_2in_24h".to_string(), JsonValue::from(val));
        }
        if let Some(val) = get_float(26, 2).map(|f| f as i32) {
            fopr_metadata.insert("storms_gt_3in_24h".to_string(), JsonValue::from(val));
        }

        // Frequency statistics (rows 31-36, 0-indexed: 30-35)
        add_frequency_stat(&mut fopr_metadata, "15min", &get_float, &get_date, 30);
        add_frequency_stat(&mut fopr_metadata, "1hr", &get_float, &get_date, 31);
        add_frequency_stat(&mut fopr_metadata, "3hr", &get_float, &get_date, 32);
        add_frequency_stat(&mut fopr_metadata, "6hr", &get_float, &get_date, 33);
        add_frequency_stat(&mut fopr_metadata, "24hr", &get_float, &get_date, 34);
        add_frequency_stat(&mut fopr_metadata, "72hr", &get_float, &get_date, 35);

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

/// Parse gage ID history: "59700; 4695 prior to 2/20/2018"
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

    GageIdHistory {
        current_id,
        previous_ids,
    }
}

/// Convert Excel date serial to NaiveDate
///
/// This is a fallback for when we get a raw f64 value instead of ExcelDateTime.
/// Excel stores dates as integers (serial numbers) since Dec 31, 1899.
/// Note: Prefer using ExcelDateTime::as_datetime() when available.
pub fn excel_serial_to_date(serial: f64) -> Option<NaiveDate> {
    // Excel epoch: 1899-12-30 (adjusted for Excel's off-by-one bug)
    let epoch = NaiveDate::from_ymd_opt(1899, 12, 30)?;
    epoch.checked_add_signed(Duration::days(serial as i64))
}

/// Convert ExcelDateTime to NaiveDate using calamine's built-in conversion
fn excel_datetime_to_date(dt: &calamine::ExcelDateTime) -> Option<NaiveDate> {
    dt.as_datetime().map(|chrono_dt| chrono_dt.date())
}

/// Calculate installation date from years since installation
fn calculate_installation_date(years_since: f64, reference_serial: f64) -> Option<NaiveDate> {
    let reference_date = excel_serial_to_date(reference_serial)?;
    let days_offset = (years_since * 365.25) as i64;
    reference_date.checked_sub_signed(Duration::days(days_offset))
}

/// Extract complete years count from label text
///
/// Example: "Average Annual Precipitation for 26 Complete Years (in):" → 26
fn extract_complete_years(label: &str) -> Option<i32> {
    let re = Regex::new(r"for\s+(\d+)\s+Complete Years").ok()?;
    re.captures(label)
        .and_then(|cap| cap.get(1))
        .and_then(|m| m.as_str().parse::<i32>().ok())
}

/// Parse elevation from formats like "1,465 ft." or "1320 ft."
fn parse_elevation(value: &str) -> Option<i32> {
    value
        .replace(",", "") // Remove commas
        .split_whitespace() // Split on whitespace
        .next() // Take first token
        .and_then(|s| s.parse::<i32>().ok())
}

/// Add frequency statistic to JSONB metadata
fn add_frequency_stat<F, D>(
    metadata: &mut serde_json::Map<String, JsonValue>,
    period: &str,
    get_float: &F,
    get_date: &D,
    row: usize,
) where
    F: Fn(usize, usize) -> Option<f64>,
    D: Fn(usize, usize) -> Option<NaiveDate>,
{
    // Column B (index 1): inches
    if let Some(inches) = get_float(row, 1) {
        metadata.insert(format!("freq_{period}_inches"), JsonValue::from(inches));
    }

    // Column C (index 2): date
    if let Some(date) = get_date(row, 2) {
        metadata.insert(
            format!("freq_{period}_date"),
            JsonValue::from(date.to_string()),
        );
    }

    // Column D (index 3): return period (years)
    if let Some(years) = get_float(row, 3) {
        metadata.insert(
            format!("freq_{period}_return_period_yrs"),
            JsonValue::from(years as i32),
        );
    }
}

/// Validate latitude is within Maricopa County bounds
fn validate_latitude(lat: f64) -> Result<(), ParseError> {
    if (32.0..=34.0).contains(&lat) {
        Ok(())
    } else {
        Err(ParseError::ValidationError(format!(
            "Latitude {lat} outside Maricopa County range (32.0 - 34.0)"
        )))
    }
}

/// Validate longitude is within Maricopa County bounds
fn validate_longitude(lon: f64) -> Result<(), ParseError> {
    if (-113.0..=-111.0).contains(&lon) {
        Ok(())
    } else {
        Err(ParseError::ValidationError(format!(
            "Longitude {lon} outside Maricopa County range (-113.0 - -111.0)"
        )))
    }
}

/// Validate elevation is within reasonable range
fn validate_elevation(elev: i32) -> Result<(), ParseError> {
    if (500..=4000).contains(&elev) {
        Ok(())
    } else {
        Err(ParseError::ValidationError(format!(
            "Elevation {elev} outside reasonable range (500 - 4000 ft)"
        )))
    }
}

/// Validate precipitation is within reasonable range
fn validate_precipitation(inches: f64) -> Result<(), ParseError> {
    if (0.0..=20.0).contains(&inches) {
        Ok(())
    } else {
        Err(ParseError::ValidationError(format!(
            "Precipitation {inches} outside reasonable range (0.0 - 20.0 inches)"
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

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
    fn test_parse_gage_id_history_multiple_previous() {
        let input = "59700; 4695 prior to 2/20/2018; 1234 prior to 1/1/2010";
        let result = parse_gage_id_history(input);
        assert_eq!(result.current_id, "59700");
        assert_eq!(result.previous_ids, vec!["4695", "1234"]);
    }

    #[test]
    fn test_excel_serial_to_date() {
        // 35835 = February 9, 1998
        let date = excel_serial_to_date(35835.0).unwrap();
        assert_eq!(date.year(), 1998);
        assert_eq!(date.month(), 2);
        assert_eq!(date.day(), 9);
    }

    #[test]
    fn test_excel_serial_to_date_water_year_start() {
        // 45566 = October 1, 2024
        let date = excel_serial_to_date(45566.0).unwrap();
        assert_eq!(date.year(), 2024);
        assert_eq!(date.month(), 10);
        assert_eq!(date.day(), 1);
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
    fn test_parse_elevation_invalid() {
        assert_eq!(parse_elevation("invalid"), None);
    }

    #[test]
    fn test_extract_complete_years() {
        let label = "Average Annual Precipitation for 26 Complete Years (in):";
        assert_eq!(extract_complete_years(label), Some(26));
    }

    #[test]
    fn test_extract_complete_years_different_count() {
        let label = "Average Annual Precipitation for 27 Complete Years (in):";
        assert_eq!(extract_complete_years(label), Some(27));
    }

    #[test]
    fn test_extract_complete_years_no_match() {
        let label = "Some other label";
        assert_eq!(extract_complete_years(label), None);
    }

    #[test]
    fn test_validate_latitude_valid() {
        assert!(validate_latitude(33.61006).is_ok());
    }

    #[test]
    fn test_validate_latitude_too_far_north() {
        assert!(validate_latitude(40.0).is_err());
    }

    #[test]
    fn test_validate_latitude_too_far_south() {
        assert!(validate_latitude(30.0).is_err());
    }

    #[test]
    fn test_validate_longitude_valid() {
        assert!(validate_longitude(-111.86545).is_ok());
    }

    #[test]
    fn test_validate_longitude_too_far_east() {
        assert!(validate_longitude(-100.0).is_err());
    }

    #[test]
    fn test_validate_longitude_too_far_west() {
        assert!(validate_longitude(-115.0).is_err());
    }

    #[test]
    fn test_validate_elevation_valid() {
        assert!(validate_elevation(1465).is_ok());
    }

    #[test]
    fn test_validate_elevation_too_low() {
        assert!(validate_elevation(100).is_err());
    }

    #[test]
    fn test_validate_elevation_too_high() {
        assert!(validate_elevation(5000).is_err());
    }

    #[test]
    fn test_validate_precipitation_valid() {
        assert!(validate_precipitation(7.48).is_ok());
    }

    #[test]
    fn test_validate_precipitation_negative() {
        assert!(validate_precipitation(-1.0).is_err());
    }

    #[test]
    fn test_validate_precipitation_too_high() {
        assert!(validate_precipitation(25.0).is_err());
    }

    #[test]
    fn test_calculate_installation_date() {
        // Reference: Oct 1, 2024 (45566)
        // Years since: 26.642026009582477
        // Expected: ~Feb 1998 (26.64 years before Oct 1, 2024)
        let install_date = calculate_installation_date(26.642026009582477, 45566.0).unwrap();
        assert_eq!(install_date.year(), 1998);
        // Allow Jan-Mar range due to calculation method
        assert!(install_date.month() >= 1 && install_date.month() <= 3);
    }
}

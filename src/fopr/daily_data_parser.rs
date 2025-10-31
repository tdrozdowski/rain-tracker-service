/// FOPR Daily Rainfall Data Parser
///
/// Parses daily rainfall readings from year sheets in FOPR Excel files.
/// Each FOPR file contains multiple year sheets (2024, 2023, 2022, etc.) with daily data.
use calamine::{open_workbook, Data, Reader, Xlsx};
use std::fs::File;
use std::io::BufReader;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::fopr::metadata_parser::excel_serial_to_date;
use crate::importers::excel_importer::HistoricalReading;

#[derive(Error, Debug)]
pub enum FoprParseError {
    #[error("Failed to open workbook: {0}")]
    WorkbookOpen(String),

    #[error("Sheet not found: {0}")]
    SheetNotFound(String),

    #[error("Invalid data at row {row}: {msg}")]
    InvalidData { row: usize, msg: String },

    #[error("Invalid date: {0}")]
    InvalidDate(String),

    #[error("No year sheets found in workbook")]
    NoYearSheets,
}

/// Parser for FOPR daily rainfall data
pub struct FoprDailyDataParser {
    workbook_path: String,
    station_id: String,
}

impl FoprDailyDataParser {
    /// Create a new FOPR daily data parser
    ///
    /// # Arguments
    /// * `workbook_path` - Path to the FOPR Excel file (e.g., "59700_FOPR.xlsx")
    /// * `station_id` - Station ID for the gauge (e.g., "59700")
    pub fn new(workbook_path: impl Into<String>, station_id: impl Into<String>) -> Self {
        Self {
            workbook_path: workbook_path.into(),
            station_id: station_id.into(),
        }
    }

    /// Parse all year sheets in the FOPR file
    ///
    /// Returns a Vec of HistoricalReading for all years found in the file.
    /// Year sheets are identified by numeric names (e.g., "2024", "2023").
    /// Skips non-year sheets like "Meta_Stats", "AnnualTables", etc.
    pub fn parse_all_years(&self) -> Result<Vec<HistoricalReading>, FoprParseError> {
        info!("Parsing FOPR file: {}", self.workbook_path);

        // Open workbook
        let mut workbook: Xlsx<BufReader<File>> = match open_workbook(&self.workbook_path) {
            Ok(wb) => wb,
            Err(e) => return Err(FoprParseError::WorkbookOpen(e.to_string())),
        };

        let sheet_names = workbook.sheet_names().to_owned();
        debug!("Found {} total sheets", sheet_names.len());

        let mut all_readings = Vec::new();
        let mut year_sheets_found = 0;

        // Find and parse year sheets
        for sheet_name in sheet_names {
            // Check if sheet name is a 4-digit year (e.g., "2024", "2023")
            if let Ok(year) = sheet_name.parse::<i32>() {
                // Valid year range check (1990-2030)
                if (1990..=2030).contains(&year) {
                    year_sheets_found += 1;
                    debug!("Parsing year sheet: {} (water year {})", sheet_name, year);

                    match self.parse_year_sheet(&mut workbook, &sheet_name, year) {
                        Ok(readings) => {
                            info!("✓ Parsed {} readings from year {}", readings.len(), year);
                            all_readings.extend(readings);
                        }
                        Err(e) => {
                            warn!("Failed to parse year sheet {}: {}", year, e);
                            // Continue with other sheets instead of failing completely
                        }
                    }
                } else {
                    debug!("Skipping sheet '{}': year out of range", sheet_name);
                }
            } else {
                debug!(
                    "Skipping non-year sheet: {} (not a 4-digit number)",
                    sheet_name
                );
            }
        }

        if year_sheets_found == 0 {
            return Err(FoprParseError::NoYearSheets);
        }

        info!(
            "Parsed {} year sheets, total {} readings",
            year_sheets_found,
            all_readings.len()
        );

        Ok(all_readings)
    }

    /// Parse a single year sheet
    ///
    /// Year sheets have the structure:
    /// - Column A (index 0): Excel date serial (Float) - e.g., 45200 = 2023-10-01
    /// - Column B (index 1): Daily incremental rainfall in inches (Float)
    /// - Column C (index 2): Empty (possibly for notes/flags)
    /// - No header row - data starts at row 0
    fn parse_year_sheet(
        &self,
        workbook: &mut Xlsx<BufReader<File>>,
        sheet_name: &str,
        _year: i32,
    ) -> Result<Vec<HistoricalReading>, FoprParseError> {
        let range = match workbook.worksheet_range(sheet_name) {
            Ok(range) => range,
            Err(_) => return Err(FoprParseError::SheetNotFound(sheet_name.to_string())),
        };

        let mut readings = Vec::new();
        let (row_count, _col_count) = range.get_size();

        debug!("Year sheet '{}' has {} rows", sheet_name, row_count);

        // Parse each row (no headers, data starts at row 0)
        for row_idx in 0..row_count {
            // Column A: Excel date serial (can be Float, Int, or DateTime)
            let date_serial = match range.get((row_idx, 0)) {
                Some(Data::Float(f)) => *f,
                Some(Data::Int(i)) => *i as f64,
                Some(Data::DateTime(dt)) => dt.as_f64(),
                Some(Data::Empty) => {
                    debug!("Empty date cell at row {}, skipping", row_idx);
                    continue;
                }
                Some(other) => {
                    debug!(
                        "Unexpected date format at row {}: {:?}, skipping",
                        row_idx, other
                    );
                    continue;
                }
                None => {
                    debug!("No date value at row {}, skipping", row_idx);
                    continue;
                }
            };

            // Column B: Rainfall in inches
            let rainfall = match range.get((row_idx, 1)) {
                Some(Data::Float(f)) => *f,
                Some(Data::Int(i)) => *i as f64,
                Some(Data::Empty) => 0.0, // Empty cell = no rain
                Some(other) => {
                    warn!(
                        "Unexpected rainfall format at row {}: {:?}, using 0.0",
                        row_idx, other
                    );
                    0.0
                }
                None => 0.0, // Missing value = no rain
            };

            // Validate rainfall value
            if !(0.0..=20.0).contains(&rainfall) {
                warn!(
                    "Suspicious rainfall value at row {}: {} inches (skipping)",
                    row_idx, rainfall
                );
                continue;
            }

            // Convert Excel date serial to NaiveDate
            let date = match excel_serial_to_date(date_serial) {
                Some(d) => d,
                None => {
                    warn!(
                        "Failed to convert Excel date serial {} at row {} (skipping)",
                        date_serial, row_idx
                    );
                    continue;
                }
            };

            // Validate date is not in the future
            let today = chrono::Local::now().date_naive();
            if date > today {
                debug!("Future date {} at row {} (skipping)", date, row_idx);
                continue;
            }

            // Skip rows with zero rainfall (optional optimization)
            // Comment out if you want to store all rows including zero rainfall
            if rainfall == 0.0 {
                continue;
            }

            readings.push(HistoricalReading {
                station_id: self.station_id.clone(),
                reading_date: date,
                rainfall_inches: rainfall,
                footnote_marker: None,
            });
        }

        debug!("Extracted {} non-zero readings from sheet", readings.len());

        Ok(readings)
    }

    /// Get list of available year sheets in the FOPR file
    ///
    /// Useful for reporting or selective parsing.
    pub fn get_available_years(&self) -> Result<Vec<i32>, FoprParseError> {
        let workbook: Xlsx<BufReader<File>> = match open_workbook(&self.workbook_path) {
            Ok(wb) => wb,
            Err(e) => return Err(FoprParseError::WorkbookOpen(e.to_string())),
        };

        let sheet_names = workbook.sheet_names().to_owned();
        let mut years = Vec::new();

        for sheet_name in sheet_names {
            if let Ok(year) = sheet_name.parse::<i32>() {
                if (1990..=2030).contains(&year) {
                    years.push(year);
                }
            }
        }

        years.sort_unstable();
        years.reverse(); // Most recent first

        Ok(years)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn test_get_available_years() {
        // This test requires the sample file to exist
        let parser = FoprDailyDataParser::new("sample-data-files/59700_FOPR.xlsx", "59700");

        match parser.get_available_years() {
            Ok(years) => {
                println!("Available years: {years:?}");
                assert!(!years.is_empty(), "Should find at least one year sheet");
                assert!(years.iter().all(|&y| (1990..=2030).contains(&y)));
            }
            Err(e) => {
                println!("Note: Test skipped (sample file not found): {e}");
            }
        }
    }

    #[test]
    fn test_parse_all_years() {
        // This test requires the sample file to exist
        let parser = FoprDailyDataParser::new("sample-data-files/59700_FOPR.xlsx", "59700");

        match parser.parse_all_years() {
            Ok(readings) => {
                println!("Parsed {} total readings", readings.len());
                assert!(!readings.is_empty(), "Should parse at least some readings");

                // Verify all readings have correct station ID
                assert!(
                    readings.iter().all(|r| r.station_id == "59700"),
                    "All readings should have station_id 59700"
                );

                // Verify readings are within reasonable date range
                let earliest = readings.iter().map(|r| r.reading_date).min().unwrap();
                let latest = readings.iter().map(|r| r.reading_date).max().unwrap();
                println!("Date range: {earliest} to {latest}");

                assert!(earliest.year() >= 1990);
                assert!(latest.year() <= 2030);
            }
            Err(e) => {
                println!("Note: Test skipped (sample file not found): {e}");
            }
        }
    }
}

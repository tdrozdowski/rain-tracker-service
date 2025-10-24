use calamine::{open_workbook, Data, Reader, Xlsx};
use chrono::NaiveDate;
use std::fs::File;
use std::io::BufReader;
use thiserror::Error;
use tracing::{debug, info, warn};

#[derive(Error, Debug)]
pub enum ExcelImportError {
    #[error("Failed to open workbook: {0}")]
    WorkbookOpen(String),

    #[error("Sheet not found: {0}")]
    SheetNotFound(String),

    #[error("Invalid data at row {row}, col {col}: {msg}")]
    InvalidData { row: usize, col: usize, msg: String },

    #[error("Missing gauge IDs in header row")]
    MissingGaugeIds,

    #[error("Invalid date format: {0}")]
    InvalidDate(String),
}

/// Represents a single rainfall reading from historical data files
#[derive(Debug, Clone)]
pub struct HistoricalReading {
    pub station_id: String,
    pub reading_date: NaiveDate,
    pub rainfall_inches: f64,
    /// Optional footnote marker from PDF (e.g., "1", "2") indicating a data quality note
    pub footnote_marker: Option<String>,
}

/// Parser for MCFCD Water Year Excel files (format: pcp_WY_YYYY.xlsx)
pub struct ExcelImporter {
    workbook_path: String,
}

impl ExcelImporter {
    pub fn new(workbook_path: impl Into<String>) -> Self {
        Self {
            workbook_path: workbook_path.into(),
        }
    }

    /// Parse a single month sheet from the water year Excel file
    ///
    /// # Expected Sheet Structure:
    /// ```
    /// Row 1: Header ("FCD of Maricopa County ALERT System")
    /// Row 2: Column numbers (1, 2, 3, ...)
    /// Row 3: Gage IDs (1000, 1200, 1500, ...)
    /// Row 4-34: Daily data (YYYY-MM-DD | rainfall values)
    /// Row 35: Monthly totals ("Totals:" | sum for each gauge)
    /// ```
    pub fn parse_month_sheet(
        &self,
        sheet_name: &str,
    ) -> Result<Vec<HistoricalReading>, ExcelImportError> {
        info!("Parsing sheet: {}", sheet_name);

        // Open workbook (this is synchronous, caller should use spawn_blocking)
        let mut workbook: Xlsx<BufReader<File>> = match open_workbook(&self.workbook_path) {
            Ok(wb) => wb,
            Err(e) => return Err(ExcelImportError::WorkbookOpen(e.to_string())),
        };

        // Get the sheet - try to get it by name
        let range = match workbook.worksheet_range(sheet_name) {
            Ok(range) => range,
            Err(_) => return Err(ExcelImportError::SheetNotFound(sheet_name.to_string())),
        };

        let mut readings = Vec::new();

        // Row 3 (index 2) contains gauge IDs
        let gauge_ids = self.parse_gauge_ids(&range, 2)?;
        debug!(
            "Found {} gauge IDs in sheet {}",
            gauge_ids.len(),
            sheet_name
        );

        // Rows 4-34 (indices 3-33) contain daily rainfall data
        // Dates are in column A (index 0), rainfall values start at column B (index 1)
        for row_idx in 3..=33 {
            // Check if we've reached the totals row
            if let Some(Data::String(s)) = range.get((row_idx, 0)) {
                if s.to_lowercase().contains("total") {
                    debug!("Reached totals row at index {}", row_idx);
                    break;
                }
            }

            // Parse date from column A
            let date = match self.parse_date(&range, row_idx, 0)? {
                Some(d) => d,
                None => {
                    debug!("No more dates at row {}, stopping", row_idx);
                    break;
                }
            };

            // Parse rainfall values for each gauge
            for (col_idx, station_id) in gauge_ids.iter().enumerate() {
                let data_col = col_idx + 1; // Offset by 1 since dates are in column 0

                if let Some(rainfall) = self.parse_rainfall(&range, row_idx, data_col)? {
                    // Only store non-zero values to save space
                    if rainfall > 0.0 {
                        readings.push(HistoricalReading {
                            station_id: station_id.clone(),
                            reading_date: date,
                            rainfall_inches: rainfall,
                            footnote_marker: None, // Excel files don't have footnotes
                        });
                    }
                }
            }
        }

        info!(
            "Parsed {} non-zero rainfall readings from sheet {}",
            readings.len(),
            sheet_name
        );
        Ok(readings)
    }

    /// Parse all month sheets in a water year Excel file
    ///
    /// Returns readings for all months in the water year (Oct - Sep)
    pub fn parse_all_months(
        &self,
        water_year: i32,
    ) -> Result<Vec<HistoricalReading>, ExcelImportError> {
        let months = [
            "OCT", "NOV", "DEC", "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP",
        ];

        let mut all_readings = Vec::new();

        for month_name in months {
            match self.parse_month_sheet(month_name) {
                Ok(mut readings) => {
                    info!(
                        "Successfully parsed {}: {} readings",
                        month_name,
                        readings.len()
                    );
                    all_readings.append(&mut readings);
                }
                Err(ExcelImportError::SheetNotFound(_)) => {
                    warn!(
                        "Sheet {} not found in water year {}, skipping",
                        month_name, water_year
                    );
                    continue;
                }
                Err(e) => return Err(e),
            }
        }

        info!(
            "Parsed total of {} readings from water year {}",
            all_readings.len(),
            water_year
        );
        Ok(all_readings)
    }

    /// Parse gauge IDs from Row 3
    fn parse_gauge_ids(
        &self,
        range: &calamine::Range<Data>,
        row: usize,
    ) -> Result<Vec<String>, ExcelImportError> {
        let mut gauge_ids = Vec::new();

        // Start from column 1 (index 1) since column 0 is the date column header
        for col in 1..range.width() {
            match range.get((row, col)) {
                Some(Data::Int(i)) => {
                    gauge_ids.push(i.to_string());
                }
                Some(Data::Float(f)) => {
                    gauge_ids.push(format!("{f:.0}"));
                }
                Some(Data::String(s)) => {
                    if !s.trim().is_empty() {
                        gauge_ids.push(s.trim().to_string());
                    } else {
                        break;
                    }
                }
                Some(Data::Empty) | None => break,
                _ => {
                    warn!("Unexpected data type at row {}, col {}", row, col);
                }
            }
        }

        if gauge_ids.is_empty() {
            return Err(ExcelImportError::MissingGaugeIds);
        }

        Ok(gauge_ids)
    }

    /// Parse a date from the specified cell (expected format: YYYY-MM-DD or Excel date serial)
    fn parse_date(
        &self,
        range: &calamine::Range<Data>,
        row: usize,
        col: usize,
    ) -> Result<Option<NaiveDate>, ExcelImportError> {
        match range.get((row, col)) {
            Some(Data::String(s)) => {
                // Parse ISO date format: YYYY-MM-DD
                NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d")
                    .map(Some)
                    .map_err(|_| ExcelImportError::InvalidDate(s.clone()))
            }
            Some(Data::DateTime(excel_date)) => {
                // Excel DateTime - calamine provides direct conversion
                let timestamp = excel_date.as_datetime();
                Ok(timestamp.map(|dt| dt.date()))
            }
            Some(Data::Float(f)) => {
                // Excel date serial number
                let days = *f as i64;
                let base_date = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
                Ok(Some(base_date + chrono::Duration::days(days)))
            }
            Some(Data::Int(i)) => {
                // Excel date serial number
                let base_date = NaiveDate::from_ymd_opt(1899, 12, 30).unwrap();
                Ok(Some(base_date + chrono::Duration::days(*i)))
            }
            Some(Data::Empty) | None => Ok(None),
            other => Err(ExcelImportError::InvalidData {
                row,
                col,
                msg: format!("Expected date, got: {other:?}"),
            }),
        }
    }

    /// Parse rainfall value from the specified cell
    fn parse_rainfall(
        &self,
        range: &calamine::Range<Data>,
        row: usize,
        col: usize,
    ) -> Result<Option<f64>, ExcelImportError> {
        match range.get((row, col)) {
            Some(Data::Float(f)) => Ok(Some(*f)),
            Some(Data::Int(i)) => Ok(Some(*i as f64)),
            Some(Data::String(s)) => {
                let trimmed = s.trim();
                // Skip empty, underscore, or N/A values (gauge outage)
                if trimmed.is_empty()
                    || trimmed == "_"
                    || trimmed.starts_with("_")
                    || trimmed.eq_ignore_ascii_case("n/a")
                {
                    Ok(None)
                } else {
                    trimmed
                        .parse::<f64>()
                        .map(Some)
                        .map_err(|_| ExcelImportError::InvalidData {
                            row,
                            col,
                            msg: format!("Cannot parse rainfall value: {s}"),
                        })
                }
            }
            Some(Data::Empty) | None => Ok(None),
            other => Err(ExcelImportError::InvalidData {
                row,
                col,
                msg: format!("Expected number, got: {other:?}"),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_excel_importer_creation() {
        let importer = ExcelImporter::new("test.xlsx");
        assert_eq!(importer.workbook_path, "test.xlsx");
    }
}

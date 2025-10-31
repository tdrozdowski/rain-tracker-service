use chrono::{Datelike, NaiveDate};
use pdf_extract::extract_text;
use std::path::Path;
use thiserror::Error;
use tracing::{debug, info, warn};

use crate::importers::HistoricalReading;

#[derive(Error, Debug)]
pub enum PdfImportError {
    #[error("Failed to extract text from PDF: {0}")]
    PdfExtraction(String),

    #[error("Failed to parse date: {0}")]
    DateParse(String),

    #[error("Invalid PDF structure: {0}")]
    InvalidStructure(String),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parser for MCFCD monthly PDF files (format: pcpMMYY.pdf)
pub struct PdfImporter {
    pdf_path: String,
}

impl PdfImporter {
    pub fn new(pdf_path: impl Into<String>) -> Self {
        Self {
            pdf_path: pdf_path.into(),
        }
    }

    /// Parse the entire PDF and extract all readings
    pub fn parse_all_pages(
        &self,
        year: i32,
        month: u32,
    ) -> Result<Vec<HistoricalReading>, PdfImportError> {
        info!("Parsing PDF: {}", self.pdf_path);

        // Extract text from PDF
        let text = extract_text(Path::new(&self.pdf_path))
            .map_err(|e| PdfImportError::PdfExtraction(e.to_string()))?;

        // Parse the extracted text
        self.parse_text(&text, year, month)
    }

    /// Parse the extracted text to find gauge groups and readings
    fn parse_text(
        &self,
        text: &str,
        year: i32,
        month: u32,
    ) -> Result<Vec<HistoricalReading>, PdfImportError> {
        let mut all_readings = Vec::new();
        let lines: Vec<&str> = text.lines().collect();

        let mut i = 0;
        while i < lines.len() {
            let line = lines[i].trim();

            // Look for gauge group headers:
            // - New format: "G001: Rain Gage Group 01"
            // - Old format: "G001: Rain Gages 0770-4505"
            if line.starts_with("G0")
                && (line.contains("Rain Gage Group") || line.contains("Rain Gages"))
            {
                debug!("Found gauge group at line {}: {}", i, line);

                // Look ahead for "Gage ID" header (might be a few lines ahead)
                let mut gage_id_line_idx = i + 1;
                while gage_id_line_idx < lines.len() && gage_id_line_idx < i + 5 {
                    if lines[gage_id_line_idx].trim().starts_with("Gage ID") {
                        break;
                    }
                    gage_id_line_idx += 1;
                }

                if gage_id_line_idx < lines.len()
                    && lines[gage_id_line_idx].trim().starts_with("Gage ID")
                {
                    let gauge_ids = self.parse_gauge_ids(lines[gage_id_line_idx])?;
                    debug!("Gauge IDs at line {}: {:?}", gage_id_line_idx, gauge_ids);

                    // Skip to "Daily precipitation values in inches" line
                    i = gage_id_line_idx + 1;
                    while i < lines.len() && !lines[i].contains("Daily precipitation") {
                        i += 1;
                    }

                    if i < lines.len() {
                        debug!("Found 'Daily precipitation' at line {}", i);
                        i += 1; // Skip the "Daily precipitation" line

                        // Skip any blank lines
                        while i < lines.len() && lines[i].trim().is_empty() {
                            i += 1;
                        }

                        debug!("Starting to parse data lines from line {}", i);

                        // Now parse the daily readings until we hit TOTALS or a new gauge group
                        let mut readings_for_group = 0;
                        while i < lines.len() {
                            let data_line = lines[i].trim();

                            // Stop conditions
                            if data_line.starts_with("TOTALS:") {
                                debug!(
                                    "Hit TOTALS at line {}, parsed {} readings for this group",
                                    i, readings_for_group
                                );
                                i += 1; // Move past TOTALS
                                break;
                            }
                            if data_line.starts_with("G0")
                                && (data_line.contains("Rain Gage Group")
                                    || data_line.contains("Rain Gages"))
                            {
                                debug!("Hit next gauge group at line {}, will process it next", i);
                                // Don't increment i, let the outer loop process this gauge group
                                break;
                            }
                            if data_line.is_empty() {
                                i += 1;
                                continue; // Skip blank lines
                            }

                            // Try to parse as a daily reading
                            match self.parse_daily_reading(data_line, &gauge_ids, year, month) {
                                Ok(readings) => {
                                    readings_for_group += readings.len();
                                    all_readings.extend(readings);
                                }
                                Err(e) => {
                                    debug!(
                                        "Failed to parse line {}: {} - error: {}",
                                        i, data_line, e
                                    );
                                }
                            }

                            i += 1;
                        }
                    } else {
                        debug!(
                            "Could not find 'Daily precipitation' line after gauge group at {}",
                            i
                        );
                        i += 1;
                    }
                } else {
                    debug!("No Gage ID header found after gauge group at line {}", i);
                    i += 1;
                }
            } else {
                // Not a gauge group line, just move to next line
                i += 1;
            }
        }

        info!(
            "Parsed {} non-zero rainfall readings from PDF",
            all_readings.len()
        );
        Ok(all_readings)
    }

    /// Parse gauge IDs from the header line
    /// Example: "Gage ID     1000     1200     1300     1500     1600     1700     1800     1900"
    fn parse_gauge_ids(&self, line: &str) -> Result<Vec<String>, PdfImportError> {
        let parts: Vec<&str> = line.split_whitespace().collect();

        // Skip "Gage ID" and collect the gauge numbers
        let gauge_ids: Vec<String> = parts
            .iter()
            .skip(2) // Skip "Gage" and "ID"
            .filter_map(|s| {
                // Parse as number to validate it's a gauge ID
                if s.parse::<u32>().is_ok() {
                    Some(s.to_string())
                } else {
                    None
                }
            })
            .collect();

        if gauge_ids.is_empty() {
            return Err(PdfImportError::InvalidStructure(
                "No gauge IDs found in header line".to_string(),
            ));
        }

        Ok(gauge_ids)
    }

    /// Parse a single daily reading line
    /// Example: "11/30/19    0.04     0.35     0.00     0.04     0.39     0.63     0.00     0.00"
    fn parse_daily_reading(
        &self,
        line: &str,
        gauge_ids: &[String],
        year: i32,
        month: u32,
    ) -> Result<Vec<HistoricalReading>, PdfImportError> {
        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.is_empty() {
            return Ok(Vec::new());
        }

        // First part should be the date in MM/DD/YY format
        let date_str = parts[0];
        let date = self.parse_date(date_str, year)?;

        // Validate the date is in the expected month
        if date.month() != month {
            warn!("Date {} is not in expected month {}, skipping", date, month);
            return Ok(Vec::new());
        }

        let mut readings = Vec::new();

        // Parse rainfall values for each gauge (skip the date, which is parts[0])
        for (idx, value_str) in parts.iter().skip(1).enumerate() {
            if idx >= gauge_ids.len() {
                break; // More values than gauge IDs, stop
            }

            // Parse rainfall value and footnote marker, handling underscores for missing data
            let (rainfall_opt, footnote_marker) = self.parse_rainfall(value_str);

            if let Some(rainfall) = rainfall_opt {
                // Only store non-zero values to save space
                if rainfall > 0.0 {
                    readings.push(HistoricalReading {
                        station_id: gauge_ids[idx].clone(),
                        reading_date: date,
                        rainfall_inches: rainfall,
                        footnote_marker,
                    });
                }
            }
        }

        Ok(readings)
    }

    /// Parse date from MM/DD/YY format
    /// Example: "11/30/19" -> NaiveDate(2019, 11, 30)
    fn parse_date(&self, date_str: &str, year: i32) -> Result<NaiveDate, PdfImportError> {
        let parts: Vec<&str> = date_str.split('/').collect();

        if parts.len() != 3 {
            return Err(PdfImportError::DateParse(format!(
                "Invalid date format: {date_str}"
            )));
        }

        let month_str = parts[0];
        let month = month_str
            .parse::<u32>()
            .map_err(|_| PdfImportError::DateParse(format!("Invalid month: {month_str}")))?;

        let day_str = parts[1];
        let day = day_str
            .parse::<u32>()
            .map_err(|_| PdfImportError::DateParse(format!("Invalid day: {day_str}")))?;

        // For year, if it's 2-digit, we need to determine the century
        // Assume 20XX for years 00-99
        let year_str = parts[2];
        let year_suffix = year_str
            .parse::<i32>()
            .map_err(|_| PdfImportError::DateParse(format!("Invalid year: {year_str}")))?;

        let full_year = if year_suffix < 100 {
            // Use the provided year parameter as a hint
            let century = (year / 100) * 100;
            century + year_suffix
        } else {
            year_suffix
        };

        NaiveDate::from_ymd_opt(full_year, month, day).ok_or_else(|| {
            PdfImportError::DateParse(format!("Invalid date: {month}/{day}/{full_year}"))
        })
    }

    /// Parse rainfall value, handling underscores for missing data and capturing footnote markers
    /// Returns: (rainfall_value, footnote_marker)
    /// Examples:
    /// - "0.04" -> (Some(0.04), None)
    /// - "____" -> (None, None) - gauge outage
    /// - "____(1)" -> (None, Some("1")) - gauge outage with footnote
    /// - "0.83(1)" -> (Some(0.83), Some("1")) - value with footnote
    /// - "0.00(2)" -> (Some(0.00), Some("2")) - value with footnote
    fn parse_rainfall(&self, value_str: &str) -> (Option<f64>, Option<String>) {
        // Check if it's missing data (underscores)
        let is_missing = value_str.starts_with('_');

        // Extract footnote marker if present
        let footnote_marker = if let Some(paren_pos) = value_str.find('(') {
            // Extract text between parentheses: "0.83(1)" -> "1"
            let after_paren = &value_str[paren_pos + 1..];
            after_paren
                .find(')')
                .map(|close_paren| after_paren[..close_paren].to_string())
        } else {
            None
        };

        // If missing data, return None for value but keep the footnote
        if is_missing {
            return (None, footnote_marker);
        }

        // Remove any footnote markers like "(1)", "(2)", etc.
        // Only strip the parenthetical notation, not the actual number
        let cleaned = if let Some(paren_pos) = value_str.find('(') {
            &value_str[..paren_pos]
        } else {
            value_str
        };

        // Try to parse as float
        let value = cleaned.trim().parse::<f64>().ok();
        (value, footnote_marker)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_date() {
        let importer = PdfImporter::new("test.pdf");

        let date = importer.parse_date("11/30/19", 2019).unwrap();
        assert_eq!(date.year(), 2019);
        assert_eq!(date.month(), 11);
        assert_eq!(date.day(), 30);
    }

    #[test]
    fn test_parse_rainfall() {
        let importer = PdfImporter::new("test.pdf");

        // Basic values without footnotes
        assert_eq!(importer.parse_rainfall("0.04"), (Some(0.04), None));
        assert_eq!(importer.parse_rainfall("1.22"), (Some(1.22), None));
        assert_eq!(importer.parse_rainfall("0.83"), (Some(0.83), None));
        assert_eq!(importer.parse_rainfall("0.31"), (Some(0.31), None));
        assert_eq!(importer.parse_rainfall("1.06"), (Some(1.06), None));
        assert_eq!(importer.parse_rainfall("0.00"), (Some(0.0), None));

        // Values with footnotes - capture both value and marker
        assert_eq!(
            importer.parse_rainfall("0.83(1)"),
            (Some(0.83), Some("1".to_string()))
        );
        assert_eq!(
            importer.parse_rainfall("0.00(2)"),
            (Some(0.0), Some("2".to_string()))
        );
        assert_eq!(
            importer.parse_rainfall("2.01(3)"),
            (Some(2.01), Some("3".to_string()))
        );

        // Missing data (underscores)
        assert_eq!(importer.parse_rainfall("____"), (None, None));
        assert_eq!(
            importer.parse_rainfall("____(1)"),
            (None, Some("1".to_string()))
        );
    }

    #[test]
    fn test_parse_gauge_ids() {
        let importer = PdfImporter::new("test.pdf");

        let line =
            "Gage ID     1000     1200     1300     1500     1600     1700     1800     1900";
        let gauge_ids = importer.parse_gauge_ids(line).unwrap();

        assert_eq!(gauge_ids.len(), 8);
        assert_eq!(gauge_ids[0], "1000");
        assert_eq!(gauge_ids[7], "1900");
    }
}

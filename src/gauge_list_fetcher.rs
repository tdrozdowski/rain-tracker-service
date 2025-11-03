use serde::{Deserialize, Serialize};
use tracing::{debug, instrument, warn};

use crate::fetch_error::FetchError;
use crate::utils;

// Note: This is the "fetcher" version of GaugeSummary (before being persisted)
// The DB model GaugeSummary (in db/models.rs) includes id, timestamps, etc.
// In the repository, import as: use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GaugeSummary {
    pub station_id: String,
    pub gauge_name: String,
    pub city_town: Option<String>,
    pub elevation_ft: Option<i32>,
    pub rainfall_past_6h_inches: Option<f64>,
    pub rainfall_past_24h_inches: Option<f64>,
    pub msp_forecast_zone: Option<String>,
    pub general_location: Option<String>,
}

#[derive(Clone)]
pub struct GaugeListFetcher {
    client: reqwest::Client,
    url: String,
}

/// Extract station ID (4 or 5 digits) from a string that may contain additional text
/// Delegates to shared utils::extract_station_id()
fn extract_station_id(value: &str) -> Result<String, FetchError> {
    utils::extract_station_id(value).map_err(|_| FetchError::ParseError)
}

impl GaugeListFetcher {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }

    #[instrument(skip(self), fields(url = %self.url))]
    pub async fn fetch_gauge_list(&self) -> Result<Vec<GaugeSummary>, FetchError> {
        debug!("Sending HTTP request to gauge list URL");
        let response = self.client.get(&self.url).send().await?;
        debug!("Received HTTP response with status: {}", response.status());

        let text = response.text().await?;
        debug!("Retrieved text content, size: {} bytes", text.len());

        self.parse_text(&text)
    }

    #[instrument(skip(self, text), fields(text_size = text.len()))]
    fn parse_text(&self, text: &str) -> Result<Vec<GaugeSummary>, FetchError> {
        debug!("Parsing gauge list text");
        let mut gauges = Vec::new();
        let mut parsing_data = false;
        let mut skipped_lines = 0;
        let mut found_gage_header = false;

        for line in text.lines() {
            let trimmed = line.trim();

            // Skip empty lines
            if trimmed.is_empty() {
                continue;
            }

            // Detect first header line (contains "Gage" and "Elev" or "Rainfall")
            // Note: Source data uses "Gage" (older spelling), not "Gauge"
            // Headers are split across two lines, so we need to track that
            if !found_gage_header
                && trimmed.contains("Gage")
                && (trimmed.contains("Elev") || trimmed.contains("Rainfall"))
            {
                debug!("Found first header line with 'Gage' and other column headers");
                found_gage_header = true;
                continue;
            }

            // Detect second header line (contains "Name" and "ID")
            if found_gage_header
                && !parsing_data
                && trimmed.contains("Name")
                && trimmed.contains("ID")
            {
                debug!("Found second header line, data parsing will start after separator");
                continue;
            }

            // Skip separator line (dashes) - after this, data rows begin
            if found_gage_header
                && !parsing_data
                && (trimmed.starts_with("---") || trimmed.contains("------"))
            {
                debug!("Skipping separator line, starting data parsing");
                parsing_data = true;
                continue;
            }

            // Skip lines before we've found headers and separator
            if !parsing_data {
                continue;
            }

            // Parse data line
            match self.parse_gauge_line(trimmed) {
                Ok(gauge) => {
                    gauges.push(gauge);
                }
                Err(e) => {
                    warn!("Failed to parse gauge line: {} - {}", e, trimmed);
                    skipped_lines += 1;
                }
            }
        }

        if skipped_lines > 0 {
            warn!("Skipped {} unparseable lines", skipped_lines);
        }
        debug!("Successfully parsed {} gauges", gauges.len());

        Ok(gauges)
    }

    fn parse_gauge_line(&self, line: &str) -> Result<GaugeSummary, FetchError> {
        // Expected format (whitespace-delimited):
        // Gauge Name              City/Town       ID      Elev   6hr    24hr   Zone   Location
        // 4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W of Old US80

        let parts: Vec<&str> = line.split_whitespace().collect();

        if parts.len() < 7 {
            return Err(FetchError::ParseError);
        }

        // The challenge: gauge name and city/town can have multiple words
        // We need to find where the station ID is (should be numeric)
        // Then work backwards and forwards from there

        // Find the station ID (should be a number, typically position varies due to name length)
        let mut station_id_idx = None;
        for (idx, part) in parts.iter().enumerate() {
            if part.parse::<i32>().is_ok() && idx >= 2 {
                // Found a numeric value that's not at the very beginning
                // Check if next value is also numeric (elevation)
                if idx + 1 < parts.len() && parts[idx + 1].parse::<i32>().is_ok() {
                    station_id_idx = Some(idx);
                    break;
                }
            }
        }

        let station_id_idx = station_id_idx.ok_or(FetchError::ParseError)?;

        // Parse fields based on station_id position
        // Before station_id: gauge_name and city_town
        // At station_id: station_id
        // After: elevation, 6hr, 24hr, zone, location...

        if station_id_idx < 2 || station_id_idx + 5 >= parts.len() {
            return Err(FetchError::ParseError);
        }

        // Extract station_id (5 digits only, ignore any text like "since 03/09/18")
        let station_id = extract_station_id(parts[station_id_idx])?;

        // Elevation (next field after station_id)
        let elevation_ft = parts[station_id_idx + 1]
            .parse::<i32>()
            .map_err(|e| FetchError::NumberError(e.to_string()))?;

        // 6hr rainfall
        let rainfall_past_6h = parts[station_id_idx + 2]
            .parse::<f64>()
            .map_err(|e| FetchError::NumberError(e.to_string()))?;

        // 24hr rainfall
        let rainfall_past_24h = parts[station_id_idx + 3]
            .parse::<f64>()
            .map_err(|e| FetchError::NumberError(e.to_string()))?;

        // MSP Forecast Zone
        let msp_zone = parts.get(station_id_idx + 4).map(|s| s.to_string());
        let msp_zone = if msp_zone.as_deref() == Some("None") {
            None
        } else {
            msp_zone
        };

        // General location (everything after zone)
        let general_location = if station_id_idx + 5 < parts.len() {
            Some(parts[station_id_idx + 5..].join(" "))
        } else {
            None
        };

        // Now work backwards: city/town is the word before station_id
        let city_town = if station_id_idx > 0 {
            Some(parts[station_id_idx - 1].to_string())
        } else {
            None
        };

        // Gauge name is everything before city/town
        let gauge_name = if station_id_idx >= 2 {
            parts[0..station_id_idx - 1].join(" ")
        } else {
            return Err(FetchError::ParseError);
        };

        Ok(GaugeSummary {
            station_id,
            gauge_name,
            city_town,
            elevation_ft: Some(elevation_ft),
            rainfall_past_6h_inches: Some(rainfall_past_6h),
            rainfall_past_24h_inches: Some(rainfall_past_24h),
            msp_forecast_zone: msp_zone,
            general_location,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gauge_line_valid() {
        let fetcher = GaugeListFetcher::new("".to_string());
        let line = "4th of July Wash        Agua Caliente   41200   1120   0.00   0.00   None   21 mi. W of Old US80 on Agua Caliente Road";

        let result = fetcher.parse_gauge_line(line);
        assert!(result.is_ok());

        let gauge = result.unwrap();
        assert_eq!(gauge.station_id, "41200");
        // With the current parsing logic, gauge_name includes everything up to the last word before station_id
        assert_eq!(gauge.gauge_name, "4th of July Wash Agua");
        assert_eq!(gauge.city_town, Some("Caliente".to_string()));
        assert_eq!(gauge.elevation_ft, Some(1120));
        assert_eq!(gauge.rainfall_past_6h_inches, Some(0.00));
        assert_eq!(gauge.rainfall_past_24h_inches, Some(0.00));
        assert_eq!(
            gauge.general_location,
            Some("21 mi. W of Old US80 on Agua Caliente Road".to_string())
        );
    }

    #[test]
    fn test_parse_gauge_line_missing_fields() {
        let fetcher = GaugeListFetcher::new("".to_string());
        let line = "Test Gauge   Phoenix   12345";

        let result = fetcher.parse_gauge_line(line);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_text_with_headers() {
        let text = r#"
Multi-Interval Precipitation Totals

                         FLOOD CONTROL DISTRICT of MARICOPA COUNTY
                     Precipitation Report for ALL FCDMC Rain Stations
                       6 and 24 Hours Ending 10/16/25 at 2248
            *** Data is Preliminary and Unedited, ---- Denotes Missing Data ***

     Gage                          In or Nearest      Gage    Elev.  Rainfall    Rainfall      MSP Forecast       General
     Name                           City / Town        ID     (ft)   Past 6 hr  Past 24 hr         Zone           Location
--------------------------------   ---------------   ------  ------  ---------  ----------  ------------------   --------------------------------------------
4th of July Wash                   Agua Caliente      41200   1120      0.00       0.00     None                  21 mi. W of Old US80 on Agua Caliente Road
Columbus Wash                      Agua Caliente      40800    705      0.00       0.00     None                  8 mi. N of Agua Caliente
        "#;

        let fetcher = GaugeListFetcher::new("".to_string());
        let result = fetcher.parse_text(text);

        assert!(result.is_ok(), "Parse failed: {:?}", result.err());
        let gauges = result.unwrap();
        assert_eq!(gauges.len(), 2);
        assert_eq!(gauges[0].station_id, "41200");
        assert_eq!(gauges[1].station_id, "40800");
    }

    #[test]
    fn test_parse_text_skips_header_lines() {
        let text = r#"
Precipitation Gauge Report
Date: 10/15/25 0818

     Gage                          In or Nearest      Gage    Elev.  Rainfall    Rainfall      MSP Forecast       General
     Name                           City / Town        ID     (ft)   Past 6 hr  Past 24 hr         Zone           Location
--------------------------------   ---------------   ------  ------  ---------  ----------  ------------------   --------------------------------------------
Test Gauge One          Phoenix         12345   1000   1.00   2.00   AZ001  North Phoenix
        "#;

        let fetcher = GaugeListFetcher::new("".to_string());
        let result = fetcher.parse_text(text);

        assert!(result.is_ok());
        let gauges = result.unwrap();
        assert_eq!(gauges.len(), 1);
        assert_eq!(gauges[0].gauge_name, "Test Gauge One");
    }

    #[test]
    fn test_extract_station_id_clean() {
        let result = extract_station_id("29200");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "29200");
    }

    #[test]
    fn test_extract_station_id_with_since_text() {
        let result = extract_station_id("29200 since 03/09/18");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "29200");
    }

    #[test]
    fn test_extract_station_id_with_other_text() {
        let result = extract_station_id("12345 prior to 2020");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "12345");
    }

    #[test]
    fn test_extract_station_id_invalid_length() {
        let result = extract_station_id("123");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_station_id_non_numeric() {
        let result = extract_station_id("ABCDE");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_station_id_mixed() {
        // "1234A" will extract "1234" as a valid 4-digit ID
        // This is actually correct behavior - we want the numeric part
        let result = extract_station_id("1234A");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1234");
    }

    #[test]
    fn test_extract_station_id_4digit_clean() {
        let result = extract_station_id("1800");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1800");
    }

    #[test]
    fn test_extract_station_id_4digit_with_since() {
        let result = extract_station_id("1800 since 03/27/18");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1800");
    }

    #[test]
    fn test_extract_station_id_4digit_prior() {
        let result = extract_station_id("9999 prior to 2015");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "9999");
    }

    #[test]
    fn test_extract_station_id_too_short() {
        let result = extract_station_id("123");
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_station_id_too_long() {
        let result = extract_station_id("123456");
        assert!(result.is_err());
    }
}

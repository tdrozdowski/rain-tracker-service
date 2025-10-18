use chrono::{DateTime, NaiveDateTime, Utc};
use scraper::{Html, Selector};
use serde::Deserialize;
use tracing::{debug, error, instrument, warn};

use crate::fetch_error::FetchError;

#[derive(Debug, Clone, Deserialize)]
pub struct RainReading {
    pub reading_datetime: DateTime<Utc>,
    pub cumulative_inches: f64,
    pub incremental_inches: f64,
}

#[derive(Clone)]
pub struct RainGaugeFetcher {
    client: reqwest::Client,
    url: String,
}

impl RainGaugeFetcher {
    pub fn new(url: String) -> Self {
        Self {
            client: reqwest::Client::new(),
            url,
        }
    }

    #[instrument(skip(self), fields(url = %self.url))]
    pub async fn fetch_readings(&self) -> Result<Vec<RainReading>, FetchError> {
        debug!("Sending HTTP request to rain gauge");
        let response = self.client.get(&self.url).send().await?;
        debug!("Received HTTP response with status: {}", response.status());

        let html = response.text().await?;
        debug!("Retrieved HTML content, size: {} bytes", html.len());

        self.parse_html(&html)
    }

    #[instrument(skip(self, html), fields(html_size = html.len()))]
    fn parse_html(&self, html: &str) -> Result<Vec<RainReading>, FetchError> {
        debug!("Parsing HTML document");
        let document = Html::parse_document(html);
        let pre_selector = Selector::parse("pre").unwrap();

        // Find the PRE tag containing the data
        let pre_element = document
            .select(&pre_selector)
            .find(|element| {
                let text = element.text().collect::<String>();
                text.contains("Date") && text.contains("Time") && text.contains("inches")
            })
            .ok_or_else(|| {
                error!("No PRE element with data table found in HTML");
                debug!("HTML preview (first 500 chars): {}", &html.chars().take(500).collect::<String>());
                FetchError::ParseError
            })?;

        debug!("Found data PRE element");
        let pre_text = pre_element.text().collect::<String>();

        let mut readings = Vec::new();
        let mut skipped_rows = 0;
        let mut row_count = 0;

        // Parse each line of the PRE content
        for line in pre_text.lines() {
            let trimmed = line.trim();

            // Skip empty lines and header lines
            if trimmed.is_empty() || trimmed.starts_with("Precipitation") || trimmed.starts_with("Date") {
                continue;
            }

            row_count += 1;

            // Split by whitespace and collect parts
            let parts: Vec<&str> = trimmed.split_whitespace().collect();

            debug!("Row {}: line='{}', parts={:?}", row_count, trimmed, parts);

            // Expected format: MM/DD/YYYY HH:MM:SS cumulative incremental
            if parts.len() >= 4 {
                let date_str = parts[0];
                let time_str = parts[1];
                let cumulative_str = parts[2];
                let incremental_str = parts[3];

                debug!(
                    "Row {}: date='{}', time='{}', cumulative='{}', incremental='{}'",
                    row_count, date_str, time_str, cumulative_str, incremental_str
                );

                match self.parse_reading(date_str, time_str, cumulative_str, incremental_str) {
                    Ok(reading) => {
                        debug!("Successfully parsed row {}", row_count);
                        readings.push(reading);
                    }
                    Err(e) => {
                        warn!(
                            "Failed to parse row {}: {} (date='{}', time='{}', cumulative='{}', incremental='{}')",
                            row_count, e, date_str, time_str, cumulative_str, incremental_str
                        );
                        skipped_rows += 1;
                    }
                }
            } else {
                debug!("Row {} has insufficient parts ({}), skipping: {}", row_count, parts.len(), trimmed);
            }
        }

        if skipped_rows > 0 {
            warn!("Skipped {} unparseable rows out of {}", skipped_rows, row_count);
        }
        debug!("Successfully parsed {} readings from {} rows", readings.len(), row_count);

        Ok(readings)
    }

    fn parse_reading(
        &self,
        date_str: &str,
        time_str: &str,
        cumulative_str: &str,
        incremental_str: &str,
    ) -> Result<RainReading, FetchError> {
        let datetime_str = format!("{} {}", date_str, time_str);
        let naive_dt = NaiveDateTime::parse_from_str(&datetime_str, "%m/%d/%Y %H:%M:%S")
            .map_err(|e| FetchError::DateTimeError(e.to_string()))?;

        let reading_datetime = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);

        let cumulative_inches = cumulative_str
            .parse::<f64>()
            .map_err(|e| FetchError::NumberError(e.to_string()))?;

        let incremental_inches = incremental_str
            .parse::<f64>()
            .map_err(|e| FetchError::NumberError(e.to_string()))?;

        Ok(RainReading {
            reading_datetime,
            cumulative_inches,
            incremental_inches,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_reading() {
        let fetcher = RainGaugeFetcher::new("".to_string());
        let result = fetcher.parse_reading("10/14/2025", "06:00:00", "1.85", "0.00");
        assert!(result.is_ok());

        let reading = result.unwrap();
        assert_eq!(reading.cumulative_inches, 1.85);
        assert_eq!(reading.incremental_inches, 0.0);
    }

    #[test]
    fn test_parse_html_with_pre_tag() {
        let html = r#"
            <HTML>
            <BODY><P>
            <PRE>
Precipitation Gage
Date       Time      inches   inches
10/14/2025 12:00:00    1.85     0.00
10/14/2025 06:00:00    1.85     0.00
10/13/2025 18:00:00    1.85     0.04
10/13/2025 15:27:31    1.81     0.04
            </PRE>
            </P></BODY>
            </HTML>
        "#;

        let fetcher = RainGaugeFetcher::new("".to_string());
        let result = fetcher.parse_html(html);
        assert!(result.is_ok());

        let readings = result.unwrap();
        assert_eq!(readings.len(), 4);
        assert_eq!(readings[0].cumulative_inches, 1.85);
        assert_eq!(readings[0].incremental_inches, 0.00);
        assert_eq!(readings[3].cumulative_inches, 1.81);
        assert_eq!(readings[3].incremental_inches, 0.04);
    }

    #[test]
    fn test_parse_html_skips_header_lines() {
        let html = r#"
            <HTML>
            <BODY><P>
            <PRE>
Precipitation Gage
Date       Time      inches   inches
10/14/2025 12:00:00    1.85     0.00

10/13/2025 18:00:00    1.85     0.04
            </PRE>
            </P></BODY>
            </HTML>
        "#;

        let fetcher = RainGaugeFetcher::new("".to_string());
        let result = fetcher.parse_html(html);
        assert!(result.is_ok());

        let readings = result.unwrap();
        assert_eq!(readings.len(), 2);
    }

    #[test]
    fn test_parse_html_no_pre_tag() {
        let html = r#"
            <HTML>
            <BODY><P>
            No pre tag here
            </P></BODY>
            </HTML>
        "#;

        let fetcher = RainGaugeFetcher::new("".to_string());
        let result = fetcher.parse_html(html);
        assert!(result.is_err());
        assert!(matches!(result, Err(FetchError::ParseError)));
    }

    #[test]
    fn test_parse_html_with_real_sample() {
        let html = include_str!("../http/httpRequests/2025-10-14T135928.200.html");

        let fetcher = RainGaugeFetcher::new("".to_string());
        let result = fetcher.parse_html(html);
        assert!(result.is_ok());

        let readings = result.unwrap();
        // The sample file has 200 data rows
        assert!(readings.len() > 100, "Expected many readings, got {}", readings.len());

        // Verify first reading
        assert_eq!(readings[0].cumulative_inches, 1.85);
        assert_eq!(readings[0].incremental_inches, 0.00);

        // Verify some parsing accuracy
        let reading_with_increment = readings.iter().find(|r| r.incremental_inches == 0.04);
        assert!(reading_with_increment.is_some(), "Should have readings with 0.04 incremental");
    }
}

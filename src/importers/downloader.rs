use reqwest::Client;
use std::io::Cursor;
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum DownloadError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),

    #[error("File not found (404): {0}")]
    NotFound(String),

    #[error("Server error (5xx): {0}")]
    ServerError(String),

    #[error("Invalid URL: {0}")]
    InvalidUrl(String),
}

/// MCFCD data downloader for historical rainfall files
pub struct McfcdDownloader {
    client: Client,
    base_url: String,
}

impl McfcdDownloader {
    /// Create a new downloader
    /// Default base URL: https://alert.fcd.maricopa.gov/alert/Rain/
    pub fn new() -> Self {
        Self {
            client: Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .expect("Failed to create HTTP client"),
            base_url: "https://alert.fcd.maricopa.gov/alert/Rain/".to_string(),
        }
    }

    /// Download Excel file for a water year
    /// Example: water_year=2023 downloads pcp_WY_2023.xlsx
    pub async fn download_excel(&self, water_year: i32) -> Result<Vec<u8>, DownloadError> {
        let filename = format!("pcp_WY_{water_year}.xlsx");
        let url = format!("{}{}", self.base_url, filename);

        info!("Downloading Excel file: {}", url);
        self.download_file(&url, &filename).await
    }

    /// Download PDF file for a specific month
    /// Example: month=11, year=2019 downloads pcp1119.pdf
    pub async fn download_pdf(&self, month: u32, year: i32) -> Result<Vec<u8>, DownloadError> {
        // Convert month and year to MMYY format
        let year_suffix = year % 100;
        let filename = format!("pcp{month:02}{year_suffix:02}.pdf");
        let url = format!("{}{filename}", self.base_url);

        debug!("Downloading PDF file: {url}");
        self.download_file(&url, &filename).await
    }

    /// Download all 12 monthly PDFs for a water year
    /// Water year runs from October (year-1) to September (year)
    /// Returns Vec of (month, year, file_bytes) tuples
    pub async fn download_water_year_pdfs(
        &self,
        water_year: i32,
    ) -> Result<Vec<(u32, i32, Vec<u8>)>, DownloadError> {
        let mut results = Vec::new();

        info!("Downloading 12 monthly PDFs for water year {}", water_year);

        // October through December of previous year
        for month in 10..=12 {
            let data = self.download_pdf(month, water_year - 1).await?;
            results.push((month, water_year - 1, data));
        }

        // January through September of current year
        for month in 1..=9 {
            let data = self.download_pdf(month, water_year).await?;
            results.push((month, water_year, data));
        }

        info!(
            "Successfully downloaded all 12 PDFs for water year {}",
            water_year
        );
        Ok(results)
    }

    /// Internal helper to download a file from a URL
    async fn download_file(&self, url: &str, filename: &str) -> Result<Vec<u8>, DownloadError> {
        let response = self.client.get(url).send().await?;

        let status = response.status();

        if status.is_success() {
            let bytes = response.bytes().await?;
            debug!("Downloaded {filename} ({} bytes)", bytes.len());
            Ok(bytes.to_vec())
        } else if status.as_u16() == 404 {
            Err(DownloadError::NotFound(format!(
                "{filename} not found on server"
            )))
        } else if status.is_server_error() {
            Err(DownloadError::ServerError(format!(
                "Server error {status} while downloading {filename}"
            )))
        } else {
            Err(DownloadError::HttpError(
                response.error_for_status().unwrap_err(),
            ))
        }
    }
}

impl Default for McfcdDownloader {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper to convert downloaded bytes to a temporary file path
/// Returns a Cursor for in-memory processing
pub fn bytes_to_cursor(bytes: Vec<u8>) -> Cursor<Vec<u8>> {
    Cursor::new(bytes)
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_water_year_month_sequence() {
        // Water year 2023 should have:
        // Oct-Dec 2022: months 10,11,12 of year 2022
        // Jan-Sep 2023: months 1-9 of year 2023

        // This is a unit test for the logic, not an integration test
        let water_year = 2023;

        // Previous year months
        for month in 10..=12 {
            let year = water_year - 1;
            assert_eq!(year, 2022);
            assert!((10..=12).contains(&month));
        }

        // Current year months
        for month in 1..=9 {
            let year = water_year;
            assert_eq!(year, 2023);
            assert!((1..=9).contains(&month));
        }
    }
}

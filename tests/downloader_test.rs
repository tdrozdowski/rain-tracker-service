// Tests for McfcdDownloader to improve coverage
// Uses mockito for HTTP mocking

use mockito::Server;
use rain_tracker_service::importers::downloader::{
    bytes_to_cursor, DownloadError, McfcdDownloader,
};

// Helper to create a downloader with custom base URL (for mocking)
fn create_test_downloader(base_url: String) -> McfcdDownloader {
    McfcdDownloader::with_base_url(base_url)
}

#[tokio::test]
async fn test_download_excel_success() {
    let mut server = Server::new_async().await;

    // Mock successful Excel file download
    let mock = server
        .mock("GET", "/pcp_WY_2023.xlsx")
        .with_status(200)
        .with_header(
            "content-type",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        )
        .with_body(b"fake excel data")
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_excel(2023).await;

    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes, b"fake excel data");

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_excel_404() {
    let mut server = Server::new_async().await;

    // Mock 404 response
    let mock = server
        .mock("GET", "/pcp_WY_2099.xlsx")
        .with_status(404)
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_excel(2099).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        DownloadError::NotFound(msg) => {
            assert!(msg.contains("pcp_WY_2099.xlsx"));
            assert!(msg.contains("not found"));
        }
        _ => panic!("Expected NotFound error"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_excel_server_error() {
    let mut server = Server::new_async().await;

    // Mock 500 server error
    let mock = server
        .mock("GET", "/pcp_WY_2023.xlsx")
        .with_status(500)
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_excel(2023).await;

    assert!(result.is_err());
    match result.unwrap_err() {
        DownloadError::ServerError(msg) => {
            assert!(msg.contains("500"));
            assert!(msg.contains("pcp_WY_2023.xlsx"));
        }
        _ => panic!("Expected ServerError"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_pdf_success() {
    let mut server = Server::new_async().await;

    // Mock successful PDF download (November 2019 -> pcp1119.pdf)
    let mock = server
        .mock("GET", "/pcp1119.pdf")
        .with_status(200)
        .with_header("content-type", "application/pdf")
        .with_body(b"fake pdf data")
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_pdf(11, 2019).await;

    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes, b"fake pdf data");

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_pdf_filename_format() {
    let mut server = Server::new_async().await;

    // Test MMYY format: month=3 (March), year=2020 -> pcp0320.pdf
    let mock = server
        .mock("GET", "/pcp0320.pdf")
        .with_status(200)
        .with_body(b"march pdf")
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_pdf(3, 2020).await;

    assert!(result.is_ok());
    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_fopr_success() {
    let mut server = Server::new_async().await;

    // Mock FOPR file download
    let mock = server
        .mock("GET", "/FOPR/59700_FOPR.xlsx")
        .with_status(200)
        .with_body(b"fake fopr data")
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_fopr("59700").await;

    assert!(result.is_ok());
    let bytes = result.unwrap();
    assert_eq!(bytes, b"fake fopr data");

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_fopr_404() {
    let mut server = Server::new_async().await;

    let mock = server
        .mock("GET", "/FOPR/99999_FOPR.xlsx")
        .with_status(404)
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_fopr("99999").await;

    assert!(result.is_err());
    match result.unwrap_err() {
        DownloadError::NotFound(_) => {}
        _ => panic!("Expected NotFound error"),
    }

    mock.assert_async().await;
}

#[tokio::test]
async fn test_download_water_year_pdfs_partial() {
    // Test downloading first 3 PDFs of a water year (Oct-Dec)
    let mut server = Server::new_async().await;

    // Mock October, November, December 2022
    let mock_oct = server
        .mock("GET", "/pcp1022.pdf")
        .with_status(200)
        .with_body(b"oct pdf")
        .create_async()
        .await;

    let mock_nov = server
        .mock("GET", "/pcp1122.pdf")
        .with_status(200)
        .with_body(b"nov pdf")
        .create_async()
        .await;

    let mock_dec = server
        .mock("GET", "/pcp1222.pdf")
        .with_status(200)
        .with_body(b"dec pdf")
        .create_async()
        .await;

    // Mock January-September 2023 (9 months)
    for month in 1..=9 {
        server
            .mock("GET", format!("/pcp{month:02}23.pdf").as_str())
            .with_status(200)
            .with_body(format!("month {month} pdf"))
            .create_async()
            .await;
    }

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_water_year_pdfs(2023).await;

    assert!(result.is_ok());
    let pdfs = result.unwrap();
    assert_eq!(pdfs.len(), 12, "Should download all 12 months");

    // Verify October-December are from 2022
    assert_eq!(pdfs[0], (10, 2022, b"oct pdf".to_vec()));
    assert_eq!(pdfs[1], (11, 2022, b"nov pdf".to_vec()));
    assert_eq!(pdfs[2], (12, 2022, b"dec pdf".to_vec()));

    // Verify January is from 2023
    assert_eq!(pdfs[3].0, 1);
    assert_eq!(pdfs[3].1, 2023);

    mock_oct.assert_async().await;
    mock_nov.assert_async().await;
    mock_dec.assert_async().await;
}

#[tokio::test]
async fn test_download_water_year_pdfs_failure_on_missing_month() {
    let mut server = Server::new_async().await;

    // Mock October successfully
    server
        .mock("GET", "/pcp1022.pdf")
        .with_status(200)
        .with_body(b"oct pdf")
        .create_async()
        .await;

    // November returns 404 - should fail entire operation
    server
        .mock("GET", "/pcp1122.pdf")
        .with_status(404)
        .create_async()
        .await;

    let downloader = create_test_downloader(server.url() + "/");
    let result = downloader.download_water_year_pdfs(2023).await;

    // Should fail because November is missing
    assert!(result.is_err());
    match result.unwrap_err() {
        DownloadError::NotFound(_) => {}
        e => panic!("Expected NotFound error, got: {e:?}"),
    }
}

#[tokio::test]
async fn test_default_impl() {
    // Test that Default trait is implemented
    let downloader = McfcdDownloader::default();
    let production_downloader = McfcdDownloader::new();

    // Both should be equivalent (Default calls new())
    // We can't access base_url directly, but we can verify construction succeeds
    let _ = downloader;
    let _ = production_downloader;
}

#[test]
fn test_bytes_to_cursor() {
    // Test the helper function
    let data = vec![1, 2, 3, 4, 5];
    let cursor = bytes_to_cursor(data.clone());

    assert_eq!(cursor.into_inner(), data);
}

#[test]
fn test_error_display() {
    // Test that error types implement Display properly
    let err = DownloadError::NotFound("test.xlsx".to_string());
    assert!(err.to_string().contains("test.xlsx"));
    assert!(err.to_string().contains("404"));

    let err = DownloadError::ServerError("500 error".to_string());
    assert!(err.to_string().contains("500"));
    assert!(err.to_string().contains("5xx"));
}

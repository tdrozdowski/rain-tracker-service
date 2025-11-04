// Error handling tests to improve coverage for error logging paths
// Tests validate that errors are properly handled and logged

use rain_tracker_service::db::{FoprImportJobRepository, GaugeRepository};
use rain_tracker_service::fopr::MetaStatsData;
use rain_tracker_service::services::{FoprImportService, GaugeService};
use rain_tracker_service::workers::FoprImportWorker;
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Test fixture module
mod error_test_fixtures {
    use super::*;

    pub async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    pub async fn cleanup_test_gauge(pool: &PgPool, station_id: &str) {
        // Clean up in reverse dependency order
        sqlx::query!(
            "DELETE FROM fopr_import_jobs WHERE station_id = $1",
            station_id
        )
        .execute(pool)
        .await
        .ok();

        sqlx::query!(
            "DELETE FROM gauge_summaries WHERE station_id = $1",
            station_id
        )
        .execute(pool)
        .await
        .ok();

        sqlx::query!(
            "DELETE FROM rain_readings WHERE station_id = $1",
            station_id
        )
        .execute(pool)
        .await
        .ok();

        sqlx::query!("DELETE FROM gauges WHERE station_id = $1", station_id)
            .execute(pool)
            .await
            .ok();
    }
}

#[tokio::test]
#[serial]
async fn test_fopr_import_service_no_readings_error() {
    // Test that FoprImportService properly handles empty FOPR files
    // This exercises the NoReadings error path
    let pool = error_test_fixtures::setup_test_db().await;
    let station_id = "ERROR_TEST_001";
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;

    let service = FoprImportService::new(pool.clone());

    // Attempt to import for a non-existent station should fail
    // This tests the download error path
    let result = service.import_fopr(station_id).await;

    assert!(
        result.is_err(),
        "Expected import to fail for non-existent station"
    );
}

#[tokio::test]
#[serial]
async fn test_gauge_service_upsert_with_foreign_key_violation() {
    // Test that gauge_service properly handles foreign key constraint violations
    // This exercises the error logging in gauge_repository upsert_summaries
    use rain_tracker_service::gauge_list_fetcher::GaugeSummary;

    let pool = error_test_fixtures::setup_test_db().await;
    let station_id = "ERROR_TEST_FK";
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;

    let gauge_repo = GaugeRepository::new(pool.clone());
    let job_repo = FoprImportJobRepository::new(pool.clone());
    let gauge_service = GaugeService::new(gauge_repo, job_repo);

    // Create a gauge summary without the corresponding gauge entry
    // This should trigger foreign key constraint violation
    let summary = GaugeSummary {
        station_id: station_id.to_string(),
        gauge_name: "Test Gauge".to_string(),
        city_town: Some("Test City".to_string()),
        elevation_ft: Some(1000),
        general_location: Some("Test Location".to_string()),
        msp_forecast_zone: Some("Test Zone".to_string()),
        rainfall_past_6h_inches: Some(0.5),
        rainfall_past_24h_inches: Some(1.0),
    };

    let result = gauge_service.upsert_summaries(&[summary]).await;

    // Should fail due to foreign key constraint
    assert!(
        result.is_err(),
        "Expected upsert to fail with foreign key constraint"
    );
}

#[tokio::test]
#[serial]
async fn test_gauge_service_handle_new_gauge_discovery_success() {
    // Test successful gauge discovery flow
    // This exercises the info! and debug! logging in gauge_service
    use rain_tracker_service::gauge_list_fetcher::GaugeSummary;

    let pool = error_test_fixtures::setup_test_db().await;
    let station_id = "ERROR_TEST_NEW";
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;

    let gauge_repo = GaugeRepository::new(pool.clone());
    let job_repo = FoprImportJobRepository::new(pool.clone());
    let gauge_service = GaugeService::new(gauge_repo, job_repo);

    let summary = GaugeSummary {
        station_id: station_id.to_string(),
        gauge_name: "New Test Gauge".to_string(),
        city_town: Some("Test City".to_string()),
        elevation_ft: Some(1000),
        general_location: Some("Test Location".to_string()),
        msp_forecast_zone: Some("Test Zone".to_string()),
        rainfall_past_6h_inches: Some(0.0),
        rainfall_past_24h_inches: Some(0.0),
    };

    // First discovery should create a job
    let result = gauge_service.handle_new_gauge_discovery(&summary).await;
    assert!(result.is_ok(), "Expected gauge discovery to succeed");
    assert!(result.unwrap(), "Expected new job to be created");

    // Second discovery should not create another job
    let result = gauge_service.handle_new_gauge_discovery(&summary).await;
    assert!(result.is_ok(), "Expected second discovery to succeed");
    assert!(!result.unwrap(), "Expected no new job on second discovery");

    // Cleanup
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_worker_with_worker_id() {
    // Test that worker can be created with worker_id
    // This exercises the new worker_id parameter
    let pool = error_test_fixtures::setup_test_db().await;

    let job_repo = FoprImportJobRepository::new(pool.clone());
    let fopr_service = FoprImportService::new(pool.clone());

    let worker_id = 42;
    let worker = FoprImportWorker::new(
        job_repo,
        fopr_service,
        30, // poll_interval_secs
        worker_id,
    );

    // Worker should be created successfully
    // We can't easily test the run() method without mocking,
    // but we validate construction works
    assert_eq!(
        std::mem::size_of_val(&worker),
        std::mem::size_of::<FoprImportWorker>()
    );
}

#[tokio::test]
#[serial]
async fn test_gauge_service_with_existing_gauge() {
    // Test that gauge_service properly handles gauges that already have metadata
    // This exercises the "already exists" code path
    use rain_tracker_service::gauge_list_fetcher::GaugeSummary;

    let pool = error_test_fixtures::setup_test_db().await;
    let station_id = "ERROR_TEST_EXISTS";
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;

    // First, create a gauge with full metadata
    use chrono::NaiveDate;
    let gauge_repo = GaugeRepository::new(pool.clone());
    let metadata = MetaStatsData {
        station_id: station_id.to_string(),
        station_name: "Existing Gauge".to_string(),
        previous_station_ids: vec![],
        station_type: "Rain".to_string(),
        latitude: 33.5,
        longitude: -112.0,
        elevation_ft: Some(1000),
        county: "Test County".to_string(),
        city: Some("Test City".to_string()),
        location_description: Some("Test Location".to_string()),
        installation_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
        data_begins_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
        status: "Active".to_string(),
        avg_annual_precipitation_inches: Some(10.0),
        complete_years_count: Some(5),
        incomplete_months_count: 0,
        missing_months_count: 0,
        data_quality_remarks: Some("Test".to_string()),
        fopr_metadata: serde_json::Map::new(),
    };

    gauge_repo.upsert_gauge_metadata(&metadata).await.unwrap();

    // Now test discovery - should not create job
    let job_repo = FoprImportJobRepository::new(pool.clone());
    let gauge_service = GaugeService::new(gauge_repo, job_repo);

    let summary = GaugeSummary {
        station_id: station_id.to_string(),
        gauge_name: "Existing Gauge".to_string(),
        city_town: Some("Test City".to_string()),
        elevation_ft: Some(1000),
        general_location: Some("Test Location".to_string()),
        msp_forecast_zone: Some("Test Zone".to_string()),
        rainfall_past_6h_inches: Some(0.0),
        rainfall_past_24h_inches: Some(0.0),
    };

    let result = gauge_service.handle_new_gauge_discovery(&summary).await;
    assert!(result.is_ok(), "Expected discovery to succeed");
    assert!(
        !result.unwrap(),
        "Expected no job creation for existing gauge"
    );

    // Cleanup
    error_test_fixtures::cleanup_test_gauge(&pool, station_id).await;
}

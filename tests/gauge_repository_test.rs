// Tests for GaugeRepository to improve coverage
// Tests count, pagination, find_by_id, and upsert operations

use chrono::NaiveDate;
use rain_tracker_service::db::GaugeRepository;
use rain_tracker_service::fopr::MetaStatsData;
use rain_tracker_service::gauge_list_fetcher::GaugeSummary as FetchedGauge;
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

mod gauge_repository_fixtures {
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

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    pub async fn cleanup(pool: &PgPool, station_id: &str) {
        sqlx::query!(
            "DELETE FROM gauge_summaries WHERE station_id = $1",
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

    pub fn create_test_fetched_gauge(station_id: &str, gauge_name: &str) -> FetchedGauge {
        FetchedGauge {
            station_id: station_id.to_string(),
            gauge_name: gauge_name.to_string(),
            city_town: Some("Test City".to_string()),
            elevation_ft: Some(1000),
            general_location: Some("Test Location".to_string()),
            msp_forecast_zone: Some("Zone 1".to_string()),
            rainfall_past_6h_inches: Some(0.5),
            rainfall_past_24h_inches: Some(1.0),
        }
    }

    pub fn create_test_metadata(station_id: &str) -> MetaStatsData {
        MetaStatsData {
            station_id: station_id.to_string(),
            station_name: format!("Test Station {station_id}"),
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
            data_quality_remarks: Some("Good quality".to_string()),
            fopr_metadata: serde_json::Map::new(),
        }
    }
}

#[tokio::test]
#[serial]
async fn test_count_empty() {
    let pool = gauge_repository_fixtures::setup_test_db().await;

    // Clean up all gauge summaries for consistent test
    sqlx::query!("DELETE FROM gauge_summaries")
        .execute(&pool)
        .await
        .ok();

    let repo = GaugeRepository::new(pool.clone());
    let count = repo.count().await.unwrap();

    assert_eq!(count, 0, "Should have 0 gauges");
}

#[tokio::test]
#[serial]
async fn test_upsert_summaries() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_TEST_001";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    // First need to create the gauge in gauges table
    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    let summaries = vec![gauge_repository_fixtures::create_test_fetched_gauge(
        station_id,
        "Test Gauge 1",
    )];

    let result = repo.upsert_summaries(&summaries).await;
    assert!(result.is_ok(), "Upsert should succeed");
    assert_eq!(result.unwrap(), 1, "Should upsert 1 summary");

    // Verify summary was created
    let found = repo.find_by_id(station_id).await.unwrap();
    assert!(found.is_some(), "Should find the gauge");
    assert_eq!(found.unwrap().gauge_name, "Test Gauge 1");

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_summaries_updates_existing() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_TEST_002";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    // Create gauge first
    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    // First upsert
    let summaries1 = vec![gauge_repository_fixtures::create_test_fetched_gauge(
        station_id,
        "Original Name",
    )];
    repo.upsert_summaries(&summaries1).await.unwrap();

    // Second upsert with updated name
    let mut gauge2 =
        gauge_repository_fixtures::create_test_fetched_gauge(station_id, "Updated Name");
    gauge2.rainfall_past_24h_inches = Some(2.0);

    let result = repo.upsert_summaries(&[gauge2]).await.unwrap();
    assert_eq!(result, 1, "Should still upsert 1");

    // Verify it was updated
    let found = repo.find_by_id(station_id).await.unwrap().unwrap();
    assert_eq!(found.gauge_name, "Updated Name");
    assert_eq!(found.rainfall_past_24h_inches, Some(2.0));

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_count_with_gauges() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_ids = ["GAUGE_COUNT_1", "GAUGE_COUNT_2", "GAUGE_COUNT_3"];

    // Clean up
    for id in &station_ids {
        gauge_repository_fixtures::cleanup(&pool, id).await;
    }

    let repo = GaugeRepository::new(pool.clone());

    // Create gauges
    for id in &station_ids {
        let metadata = gauge_repository_fixtures::create_test_metadata(id);
        repo.upsert_gauge_metadata(&metadata).await.unwrap();

        let summary =
            gauge_repository_fixtures::create_test_fetched_gauge(id, &format!("Gauge {id}"));
        repo.upsert_summaries(&[summary]).await.unwrap();
    }

    // Count should include at least our 3 gauges (may have more from other tests)
    let count = repo.count().await.unwrap();
    assert!(count >= 3, "Should have at least 3 gauges");

    // Clean up
    for id in &station_ids {
        gauge_repository_fixtures::cleanup(&pool, id).await;
    }
}

#[tokio::test]
#[serial]
async fn test_find_paginated() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_ids = [
        "GAUGE_PAGE_1",
        "GAUGE_PAGE_2",
        "GAUGE_PAGE_3",
        "GAUGE_PAGE_4",
    ];

    // Clean up
    for id in &station_ids {
        gauge_repository_fixtures::cleanup(&pool, id).await;
    }

    let repo = GaugeRepository::new(pool.clone());

    // Create gauges
    for id in &station_ids {
        let metadata = gauge_repository_fixtures::create_test_metadata(id);
        repo.upsert_gauge_metadata(&metadata).await.unwrap();

        let summary =
            gauge_repository_fixtures::create_test_fetched_gauge(id, &format!("Gauge {id}"));
        repo.upsert_summaries(&[summary]).await.unwrap();
    }

    // Test pagination
    let page1 = repo.find_paginated(0, 2).await.unwrap();
    assert!(page1.len() >= 2, "Should get at least 2 results on page 1");

    let page2 = repo.find_paginated(2, 2).await.unwrap();
    assert!(page2.len() >= 2, "Should get at least 2 results on page 2");

    // Verify different results
    if page1.len() >= 2 && page2.len() >= 2 {
        assert_ne!(
            page1[0].station_id, page2[0].station_id,
            "Pages should have different gauges"
        );
    }

    // Clean up
    for id in &station_ids {
        gauge_repository_fixtures::cleanup(&pool, id).await;
    }
}

#[tokio::test]
#[serial]
async fn test_find_by_id_not_found() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "NONEXISTENT_GAUGE";

    let repo = GaugeRepository::new(pool.clone());
    let result = repo.find_by_id(station_id).await.unwrap();

    assert!(result.is_none(), "Should not find nonexistent gauge");
}

#[tokio::test]
#[serial]
async fn test_upsert_gauge_metadata() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_META_001";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());

    let result = repo.upsert_gauge_metadata(&metadata).await;
    assert!(result.is_ok(), "Upsert metadata should succeed");

    // Verify gauge exists
    let exists = repo.gauge_exists(station_id).await.unwrap();
    assert!(exists, "Gauge should exist");

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_gauge_metadata_updates_existing() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_META_002";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    let mut metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());

    // First insert
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    // Update metadata
    metadata.station_name = "Updated Name".to_string();
    metadata.elevation_ft = Some(1500);

    let result = repo.upsert_gauge_metadata(&metadata).await;
    assert!(result.is_ok(), "Update should succeed");

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_gauge_exists() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_EXISTS_001";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    let repo = GaugeRepository::new(pool.clone());

    // Should not exist initially
    let exists_before = repo.gauge_exists(station_id).await.unwrap();
    assert!(!exists_before, "Gauge should not exist initially");

    // Create gauge
    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    // Should exist now
    let exists_after = repo.gauge_exists(station_id).await.unwrap();
    assert!(exists_after, "Gauge should exist after creation");

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_gauge_metadata_with_transaction() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_TX_001";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());

    // Test transaction method
    let mut tx = pool.begin().await.unwrap();
    let result = repo.upsert_gauge_metadata_tx(&mut tx, &metadata).await;
    assert!(result.is_ok(), "Transaction upsert should succeed");

    tx.commit().await.unwrap();

    // Verify it was committed
    let exists = repo.gauge_exists(station_id).await.unwrap();
    assert!(exists, "Gauge should exist after commit");

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_by_id_with_transaction() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_TX_002";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    // Create gauge and summary
    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    let summary = gauge_repository_fixtures::create_test_fetched_gauge(station_id, "Test Gauge");
    repo.upsert_summaries(&[summary]).await.unwrap();

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let result = repo.find_by_id_tx(&mut tx, station_id).await.unwrap();

    assert!(result.is_some(), "Should find gauge in transaction");
    tx.commit().await.unwrap();

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_gauge_exists_with_transaction() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let station_id = "GAUGE_TX_003";
    gauge_repository_fixtures::cleanup(&pool, station_id).await;

    let metadata = gauge_repository_fixtures::create_test_metadata(station_id);
    let repo = GaugeRepository::new(pool.clone());
    repo.upsert_gauge_metadata(&metadata).await.unwrap();

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let exists = repo.gauge_exists_tx(&mut tx, station_id).await.unwrap();

    assert!(exists, "Should find gauge exists in transaction");
    tx.commit().await.unwrap();

    gauge_repository_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_count_with_transaction() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let repo = GaugeRepository::new(pool.clone());

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let count = repo.count_tx(&mut tx).await.unwrap();

    assert!(count > 0, "Count should be positive");
    tx.commit().await.unwrap();
}

#[tokio::test]
#[serial]
async fn test_find_paginated_with_transaction() {
    let pool = gauge_repository_fixtures::setup_test_db().await;
    let repo = GaugeRepository::new(pool.clone());

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let results = repo.find_paginated_tx(&mut tx, 0, 10).await.unwrap();

    assert!(!results.is_empty(), "Should return results");
    tx.commit().await.unwrap();
}

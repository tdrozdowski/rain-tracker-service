// Tests for ReadingRepository to improve coverage
// Focuses on bulk insert methods and query methods

use chrono::{NaiveDate, TimeZone, Utc};
use rain_tracker_service::db::ReadingRepository;
use rain_tracker_service::importers::excel_importer::HistoricalReading;
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

mod reading_repository_fixtures {
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

    pub async fn create_test_gauge(pool: &PgPool, station_id: &str) {
        // Create a minimal gauge entry to satisfy foreign key
        sqlx::query!(
            r#"
            INSERT INTO gauges (station_id, station_name, station_type, latitude, longitude, county, status)
            VALUES ($1, $2, 'Rain', 33.5, -112.0, 'Test County', 'Active')
            ON CONFLICT (station_id) DO NOTHING
            "#,
            station_id,
            format!("Test Gauge {}", station_id)
        )
        .execute(pool)
        .await
        .ok();
    }

    pub async fn cleanup_readings(pool: &PgPool, station_id: &str) {
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
async fn test_bulk_insert_historical_readings() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_001";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    let readings = vec![
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            rainfall_inches: 0.5,
            footnote_marker: Some("*".to_string()),
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 2).unwrap(),
            rainfall_inches: 0.3,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 3).unwrap(),
            rainfall_inches: 0.8,
            footnote_marker: Some("A".to_string()),
        },
    ];

    let result = repo
        .bulk_insert_historical_readings(station_id, "test_import", &readings)
        .await;

    assert!(result.is_ok(), "Bulk insert should succeed");
    let (inserted, duplicates, affected_months) = result.unwrap();
    assert_eq!(inserted, 3, "Should insert 3 readings");
    assert_eq!(duplicates, 0, "Should have no duplicates");
    assert_eq!(affected_months.len(), 3, "Should affect 3 entries");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_bulk_insert_handles_duplicates() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_002";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    let readings = vec![HistoricalReading {
        station_id: station_id.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2025, 2, 1).unwrap(),
        rainfall_inches: 0.5,
        footnote_marker: None,
    }];

    // First insert
    let result1 = repo
        .bulk_insert_historical_readings(station_id, "test_import_1", &readings)
        .await
        .unwrap();
    assert_eq!(result1.0, 1, "First insert should succeed");

    // Second insert (duplicate)
    let result2 = repo
        .bulk_insert_historical_readings(station_id, "test_import_2", &readings)
        .await
        .unwrap();
    assert_eq!(result2.0, 0, "Should insert 0 new readings");
    assert_eq!(result2.1, 1, "Should have 1 duplicate");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_by_date_range() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_003";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    // Insert test data
    let readings = vec![
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 3, 1).unwrap(),
            rainfall_inches: 0.5,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 3, 15).unwrap(),
            rainfall_inches: 0.3,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 3, 30).unwrap(),
            rainfall_inches: 0.8,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 4, 1).unwrap(),
            rainfall_inches: 0.2,
            footnote_marker: None,
        },
    ];

    repo.bulk_insert_historical_readings(station_id, "test", &readings)
        .await
        .unwrap();

    // Query for March only
    let start = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap();

    let result = repo.find_by_date_range(station_id, start, end).await;
    assert!(result.is_ok(), "Query should succeed");

    let found_readings = result.unwrap();
    assert_eq!(found_readings.len(), 3, "Should find 3 readings in March");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_by_date_range_empty() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_004";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    // Query for date range with no data
    let start = Utc.with_ymd_and_hms(2025, 5, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();

    let result = repo.find_by_date_range(station_id, start, end).await;
    assert!(result.is_ok(), "Query should succeed even with no results");
    assert_eq!(result.unwrap().len(), 0, "Should return empty vec");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_latest() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_005";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    // Insert test data
    let readings = vec![
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap(),
            rainfall_inches: 0.5,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 15).unwrap(),
            rainfall_inches: 0.3,
            footnote_marker: None,
        },
        HistoricalReading {
            station_id: station_id.to_string(),
            reading_date: NaiveDate::from_ymd_opt(2025, 1, 30).unwrap(),
            rainfall_inches: 0.8,
            footnote_marker: None,
        },
    ];

    repo.bulk_insert_historical_readings(station_id, "test", &readings)
        .await
        .unwrap();

    let result = repo.find_latest(station_id).await;
    assert!(result.is_ok(), "Query should succeed");

    let latest = result.unwrap();
    assert!(latest.is_some(), "Should find latest reading");

    let reading = latest.unwrap();
    assert_eq!(reading.station_id, station_id);
    assert_eq!(
        reading.reading_datetime.date_naive(),
        NaiveDate::from_ymd_opt(2025, 1, 30).unwrap(),
        "Should return the latest date"
    );

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_latest_not_found() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_006";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    let result = repo.find_latest(station_id).await;
    assert!(result.is_ok(), "Query should succeed");
    assert!(result.unwrap().is_none(), "Should return None for no data");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_bulk_insert_with_transaction() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_007";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    let readings = vec![HistoricalReading {
        station_id: station_id.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2025, 6, 1).unwrap(),
        rainfall_inches: 0.5,
        footnote_marker: None,
    }];

    // Test transaction method
    let mut tx = pool.begin().await.unwrap();
    let result = repo
        .bulk_insert_historical_readings_tx(&mut tx, station_id, "test_tx", &readings)
        .await;

    assert!(result.is_ok(), "Transaction insert should succeed");
    let (inserted, duplicates, _) = result.unwrap();
    assert_eq!(inserted, 1);
    assert_eq!(duplicates, 0);

    tx.commit().await.unwrap();

    // Verify it was committed
    let latest = repo.find_latest(station_id).await.unwrap();
    assert!(latest.is_some(), "Reading should be committed");

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_by_date_range_with_transaction() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_008";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    // Insert test data
    let readings = vec![HistoricalReading {
        station_id: station_id.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2025, 7, 15).unwrap(),
        rainfall_inches: 0.5,
        footnote_marker: None,
    }];

    repo.bulk_insert_historical_readings(station_id, "test", &readings)
        .await
        .unwrap();

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let start = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();

    let result = repo
        .find_by_date_range_tx(&mut tx, station_id, start, end)
        .await;

    assert!(result.is_ok(), "Transaction query should succeed");
    assert_eq!(result.unwrap().len(), 1, "Should find 1 reading");

    tx.commit().await.unwrap();

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_find_latest_with_transaction() {
    let pool = reading_repository_fixtures::setup_test_db().await;
    let station_id = "READ_TEST_009";
    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;

    let repo = ReadingRepository::new(pool.clone());

    // Insert test data
    let readings = vec![HistoricalReading {
        station_id: station_id.to_string(),
        reading_date: NaiveDate::from_ymd_opt(2025, 8, 20).unwrap(),
        rainfall_inches: 0.5,
        footnote_marker: None,
    }];

    repo.bulk_insert_historical_readings(station_id, "test", &readings)
        .await
        .unwrap();

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let result = repo.find_latest_tx(&mut tx, station_id).await;

    assert!(result.is_ok(), "Transaction query should succeed");
    assert!(result.unwrap().is_some(), "Should find latest reading");

    tx.commit().await.unwrap();

    reading_repository_fixtures::cleanup_readings(&pool, station_id).await;
    reading_repository_fixtures::create_test_gauge(&pool, station_id).await;
}

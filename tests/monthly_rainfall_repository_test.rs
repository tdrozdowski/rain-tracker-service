// Tests for MonthlyRainfallRepository to improve coverage
// Tests upsert, query, and recalculation methods

use chrono::{NaiveDate, TimeZone, Utc};
use rain_tracker_service::db::{MonthlyRainfallRepository, ReadingRepository};
use rain_tracker_service::importers::excel_importer::HistoricalReading;
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

mod monthly_rainfall_fixtures {
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

    pub async fn cleanup(pool: &PgPool, station_id: &str) {
        sqlx::query!(
            "DELETE FROM monthly_rainfall_summary WHERE station_id = $1",
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

    /// Insert test readings for a month
    pub async fn insert_test_readings(
        pool: &PgPool,
        station_id: &str,
        year: i32,
        month: u32,
    ) -> usize {
        let reading_repo = ReadingRepository::new(pool.clone());

        let readings = vec![
            HistoricalReading {
                station_id: station_id.to_string(),
                reading_date: NaiveDate::from_ymd_opt(year, month, 1).unwrap(),
                rainfall_inches: 0.5,
                footnote_marker: None,
            },
            HistoricalReading {
                station_id: station_id.to_string(),
                reading_date: NaiveDate::from_ymd_opt(year, month, 15).unwrap(),
                rainfall_inches: 0.3,
                footnote_marker: None,
            },
            HistoricalReading {
                station_id: station_id.to_string(),
                reading_date: NaiveDate::from_ymd_opt(year, month, 28).unwrap(),
                rainfall_inches: 0.8,
                footnote_marker: None,
            },
        ];

        reading_repo
            .bulk_insert_historical_readings(station_id, "test", &readings)
            .await
            .unwrap();

        readings.len()
    }
}

#[tokio::test]
#[serial]
async fn test_upsert_monthly_summary() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_001";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings first
    let count = monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 1).await;
    assert_eq!(count, 3);

    // Get the readings from database
    let reading_repo = ReadingRepository::new(pool.clone());
    let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 2, 1, 0, 0, 0).unwrap();
    let readings = reading_repo
        .find_by_date_range(station_id, start, end)
        .await
        .unwrap();

    // Test upsert
    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());
    let result = monthly_repo
        .upsert_monthly_summary(station_id, 2025, 1, &readings)
        .await;

    assert!(result.is_ok(), "Upsert should succeed");

    // Verify summary was created
    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should have 1 summary");
    assert_eq!(summaries[0].year, 2025);
    assert_eq!(summaries[0].month, 1);
    assert_eq!(summaries[0].reading_count, 3);
    assert!((summaries[0].total_rainfall_inches - 1.6).abs() < 0.001);

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_monthly_summary_empty_readings() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_002";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());
    let result = monthly_repo
        .upsert_monthly_summary(station_id, 2025, 2, &[])
        .await;

    assert!(result.is_ok(), "Empty readings should succeed");

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_monthly_summary_updates_existing() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_003";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert initial readings
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 3).await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let start = Utc.with_ymd_and_hms(2025, 3, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap();
    let readings = reading_repo
        .find_by_date_range(station_id, start, end)
        .await
        .unwrap();

    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    // First upsert
    monthly_repo
        .upsert_monthly_summary(station_id, 2025, 3, &readings)
        .await
        .unwrap();

    // Second upsert (should update)
    let result = monthly_repo
        .upsert_monthly_summary(station_id, 2025, 3, &readings)
        .await;

    assert!(result.is_ok(), "Update should succeed");

    // Verify still only one summary
    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should still have only 1 summary");

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_get_summaries_by_date_range() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_004";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings for multiple months
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 1).await;
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 2).await;
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 3).await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    // Create summaries for each month
    for month in 1..=3 {
        let start = Utc.with_ymd_and_hms(2025, month, 1, 0, 0, 0).unwrap();
        let end = Utc.with_ymd_and_hms(2025, month + 1, 1, 0, 0, 0).unwrap();
        let readings = reading_repo
            .find_by_date_range(station_id, start, end)
            .await
            .unwrap();
        monthly_repo
            .upsert_monthly_summary(station_id, 2025, month as i32, &readings)
            .await
            .unwrap();
    }

    // Query for Q1 (Jan-Mar)
    let start = Utc.with_ymd_and_hms(2025, 1, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 4, 1, 0, 0, 0).unwrap();

    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 3, "Should find 3 monthly summaries");
    assert_eq!(summaries[0].month, 1);
    assert_eq!(summaries[1].month, 2);
    assert_eq!(summaries[2].month, 3);

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_get_summaries_by_date_range_empty() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_005";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    // Query for date range with no data
    let start = Utc.with_ymd_and_hms(2025, 5, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();

    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 0, "Should return empty vec");

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_recalculate_monthly_summary() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_006";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 6).await;

    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    // Recalculate summary
    let start = Utc.with_ymd_and_hms(2025, 6, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();

    let result = monthly_repo
        .recalculate_monthly_summary(station_id, 2025, 6, start, end)
        .await;

    assert!(result.is_ok(), "Recalculate should succeed");

    // Verify summary was created
    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should have 1 summary");
    assert_eq!(summaries[0].year, 2025);
    assert_eq!(summaries[0].month, 6);

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_upsert_monthly_summary_with_transaction() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_007";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 7).await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let start = Utc.with_ymd_and_hms(2025, 7, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
    let readings = reading_repo
        .find_by_date_range(station_id, start, end)
        .await
        .unwrap();

    // Test transaction method
    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());
    let mut tx = pool.begin().await.unwrap();

    let result = monthly_repo
        .upsert_monthly_summary_tx(&mut tx, station_id, 2025, 7, &readings)
        .await;

    assert!(result.is_ok(), "Transaction upsert should succeed");
    tx.commit().await.unwrap();

    // Verify summary was created
    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should have 1 summary");

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_get_summaries_by_date_range_with_transaction() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_008";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert and create summary
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 8).await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    let start = Utc.with_ymd_and_hms(2025, 8, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap();
    let readings = reading_repo
        .find_by_date_range(station_id, start, end)
        .await
        .unwrap();

    monthly_repo
        .upsert_monthly_summary(station_id, 2025, 8, &readings)
        .await
        .unwrap();

    // Test transaction query
    let mut tx = pool.begin().await.unwrap();
    let summaries = monthly_repo
        .get_summaries_by_date_range_tx(&mut tx, station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should find 1 summary");
    tx.commit().await.unwrap();

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_recalculate_monthly_summary_with_transaction() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_009";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 9).await;

    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    // Test transaction recalculate
    let start = Utc.with_ymd_and_hms(2025, 9, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap();

    let mut tx = pool.begin().await.unwrap();
    let result = monthly_repo
        .recalculate_monthly_summary_tx(&mut tx, station_id, 2025, 9, start, end)
        .await;

    assert!(result.is_ok(), "Transaction recalculate should succeed");
    tx.commit().await.unwrap();

    // Verify summary was created
    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1, "Should have 1 summary");

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_monthly_summary_calculations() {
    let pool = monthly_rainfall_fixtures::setup_test_db().await;
    let station_id = "MONTHLY_TEST_010";
    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
    monthly_rainfall_fixtures::create_test_gauge(&pool, station_id).await;

    // Insert readings with specific values
    monthly_rainfall_fixtures::insert_test_readings(&pool, station_id, 2025, 10).await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_repo = MonthlyRainfallRepository::new(pool.clone());

    let start = Utc.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap();
    let end = Utc.with_ymd_and_hms(2025, 11, 1, 0, 0, 0).unwrap();
    let readings = reading_repo
        .find_by_date_range(station_id, start, end)
        .await
        .unwrap();

    monthly_repo
        .upsert_monthly_summary(station_id, 2025, 10, &readings)
        .await
        .unwrap();

    let summaries = monthly_repo
        .get_summaries_by_date_range(station_id, start, end)
        .await
        .unwrap();

    assert_eq!(summaries.len(), 1);
    let summary = &summaries[0];

    // Verify calculated values
    assert_eq!(summary.reading_count, 3);
    assert!((summary.total_rainfall_inches - 1.6).abs() < 0.001); // 0.5 + 0.3 + 0.8
    assert!(summary.first_reading_date.is_some());
    assert!(summary.last_reading_date.is_some());

    monthly_rainfall_fixtures::cleanup(&pool, station_id).await;
}

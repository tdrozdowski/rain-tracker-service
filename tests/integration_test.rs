// Integration tests using transaction-based isolation for data setup
// Transactions ensure tests can run in parallel without data conflicts

mod common;

use chrono::{DateTime, Datelike, NaiveDate, TimeZone, Utc};
use rain_tracker_service::db::{GaugeRepository, MonthlyRainfallRepository, ReadingRepository};
use rain_tracker_service::fetcher::RainReading;
use rain_tracker_service::fopr::MetaStatsData;
use rain_tracker_service::services::ReadingService;
use sqlx::{Postgres, Transaction};

/// Helper to insert a test gauge using a transaction
async fn insert_test_gauge(
    tx: &mut Transaction<'_, Postgres>,
    station_id: &str,
    station_name: &str,
) {
    let gauge_repo = GaugeRepository::new(common::test_pool().await.clone());

    let metadata = MetaStatsData {
        station_id: station_id.to_string(),
        station_name: station_name.to_string(),
        previous_station_ids: vec![],
        station_type: "Rain".to_string(),
        latitude: 33.5,
        longitude: -112.0,
        elevation_ft: Some(1000),
        county: "Maricopa".to_string(),
        city: Some("Test City".to_string()),
        location_description: Some("Test Location".to_string()),
        installation_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
        data_begins_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
        status: "Active".to_string(),
        avg_annual_precipitation_inches: Some(7.5),
        complete_years_count: Some(5),
        incomplete_months_count: 0,
        missing_months_count: 0,
        data_quality_remarks: Some("Test gauge".to_string()),
        fopr_metadata: serde_json::Map::new(),
    };

    gauge_repo
        .upsert_gauge_metadata_tx(tx, &metadata)
        .await
        .expect("Failed to insert test gauge");
}

/// Helper to insert a reading using a transaction
async fn insert_reading_tx(
    tx: &mut Transaction<'_, Postgres>,
    station_id: &str,
    reading: &RainReading,
) {
    sqlx::query!(
        r#"
        INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (reading_datetime, station_id) DO NOTHING
        "#,
        reading.reading_datetime,
        reading.cumulative_inches,
        reading.incremental_inches,
        station_id
    )
    .execute(&mut **tx)
    .await
    .unwrap();
}

#[tokio::test]
async fn test_insert_and_retrieve_readings() {
    // Begin transaction for test isolation
    let mut tx = common::test_transaction().await;

    let test_station_id = "TEST_INSERT_001";

    // Insert test gauge
    insert_test_gauge(&mut tx, test_station_id, "Test Insert Gauge").await;

    // Create test readings
    let readings = vec![
        RainReading {
            reading_datetime: Utc::now(),
            cumulative_inches: 1.85,
            incremental_inches: 0.04,
        },
        RainReading {
            reading_datetime: Utc::now(),
            cumulative_inches: 1.81,
            incremental_inches: 0.04,
        },
    ];

    // Insert readings
    for reading in &readings {
        insert_reading_tx(&mut tx, test_station_id, reading).await;
    }

    // Commit transaction so service layer can see the data
    tx.commit().await.expect("Failed to commit transaction");

    // Now test with service layer (uses pool, sees committed data)
    let pool = common::test_pool().await;
    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo.clone(), monthly_rainfall_repo);

    // Retrieve latest reading
    let latest = reading_service
        .get_latest_reading(test_station_id)
        .await
        .unwrap();
    assert!(latest.is_some());

    // Cleanup (since we committed, we need manual cleanup)
    sqlx::query!("DELETE FROM rain_readings WHERE station_id = $1", test_station_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_water_year_queries() {
    // Begin transaction for test isolation
    let mut tx = common::test_transaction().await;

    let test_station_id = "TEST_WATER_YEAR_001";

    // Insert test gauge
    insert_test_gauge(&mut tx, test_station_id, "Test Water Year Gauge").await;

    // Commit so service layer can see the gauge
    tx.commit().await.expect("Failed to commit transaction");

    // Test with service layer
    let pool = common::test_pool().await;
    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo);

    // Query for current rain year
    let current_water_year = ReadingService::get_water_year(Utc::now());
    let _summary = reading_service
        .get_water_year_summary(test_station_id, current_water_year)
        .await
        .unwrap();

    // Test passes if query completes without error
}

#[tokio::test]
async fn test_water_year_total_rainfall_calculation() {
    // Use committed data since service layer needs to see it
    let pool = common::test_pool().await;
    let test_station_id = "TEST_WATER_CALC_001";

    // Setup: Insert gauge and readings
    {
        let mut tx = pool.begin().await.unwrap();

        let gauge_repo = GaugeRepository::new(pool.clone());
        let metadata = MetaStatsData {
            station_id: test_station_id.to_string(),
            station_name: "Test Water Year Calculation Gauge".to_string(),
            previous_station_ids: vec![],
            station_type: "Rain".to_string(),
            latitude: 33.5,
            longitude: -112.0,
            elevation_ft: Some(1000),
            county: "Maricopa".to_string(),
            city: Some("Test City".to_string()),
            location_description: Some("Test Location".to_string()),
            installation_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            data_begins_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            status: "Active".to_string(),
            avg_annual_precipitation_inches: Some(7.5),
            complete_years_count: Some(5),
            incomplete_months_count: 0,
            missing_months_count: 0,
            data_quality_remarks: Some("Test gauge".to_string()),
            fopr_metadata: serde_json::Map::new(),
        };
        gauge_repo.upsert_gauge_metadata_tx(&mut tx, &metadata).await.unwrap();

        // Create test readings for water year 2024
        let readings = vec![
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2023, 10, 15, 12, 0, 0).unwrap(),
                cumulative_inches: 0.5,
                incremental_inches: 0.5,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2024, 3, 15, 12, 0, 0).unwrap(),
                cumulative_inches: 2.3,
                incremental_inches: 1.8,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2024, 9, 15, 12, 0, 0).unwrap(),
                cumulative_inches: 5.75,
                incremental_inches: 3.45,
            },
        ];

        for reading in &readings {
            insert_reading_tx(&mut tx, test_station_id, reading).await;
        }

        tx.commit().await.unwrap();
    }

    // Test: Use service layer
    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());

    // Populate monthly aggregates
    for (year, month) in [(2023, 10), (2024, 3), (2024, 9)] {
        let (start, end) = month_date_range(year, month);
        monthly_rainfall_repo
            .recalculate_monthly_summary(test_station_id, year, month as i32, start, end)
            .await
            .unwrap();
    }

    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo.clone());

    // Get water year summary for 2024
    let summary = reading_service
        .get_water_year_summary(test_station_id, 2024)
        .await
        .unwrap();

    // Assert: total_rainfall_inches should equal sum of monthly incremental values
    assert_eq!(
        summary.total_rainfall_inches, 5.75,
        "Total rainfall should equal sum of monthly incremental values for the water year"
    );
    assert_eq!(summary.total_readings, 3);

    // Cleanup
    sqlx::query!("DELETE FROM monthly_rainfall_summary WHERE station_id = $1", test_station_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query!("DELETE FROM rain_readings WHERE station_id = $1", test_station_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_calendar_year_total_rainfall_calculation() {
    // Use committed data since service layer needs to see it
    let pool = common::test_pool().await;
    let test_station_id = "TEST_CAL_CALC_001";

    // Setup: Insert gauge and readings in a transaction
    {
        let mut tx = pool.begin().await.unwrap();
        insert_test_gauge(&mut tx, test_station_id, "Test Calendar Calculation Gauge").await;

        // Create test readings for calendar year 2025
        let readings = vec![
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2024, 12, 31, 12, 0, 0).unwrap(),
                cumulative_inches: 0.5,
                incremental_inches: 0.1,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2025, 1, 31, 12, 0, 0).unwrap(),
                cumulative_inches: 1.0,
                incremental_inches: 0.5,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2025, 3, 31, 12, 0, 0).unwrap(),
                cumulative_inches: 2.5,
                incremental_inches: 1.5,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2025, 9, 30, 12, 0, 0).unwrap(),
                cumulative_inches: 5.0,
                incremental_inches: 2.5,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap(),
                cumulative_inches: 0.3,
                incremental_inches: 0.3,
            },
            RainReading {
                reading_datetime: Utc.with_ymd_and_hms(2025, 12, 31, 12, 0, 0).unwrap(),
                cumulative_inches: 0.8,
                incremental_inches: 0.5,
            },
        ];

        for reading in &readings {
            insert_reading_tx(&mut tx, test_station_id, reading).await;
        }

        tx.commit().await.unwrap();
    }

    // Test: Use service layer
    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());

    // Populate monthly aggregates
    for (year, month) in [(2024, 12), (2025, 1), (2025, 3), (2025, 9), (2025, 10), (2025, 12)] {
        let (start, end) = month_date_range(year, month);
        monthly_rainfall_repo
            .recalculate_monthly_summary(test_station_id, year, month as i32, start, end)
            .await
            .unwrap();
    }

    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo.clone());

    // Get calendar year summary for 2025
    let summary = reading_service
        .get_calendar_year_summary(test_station_id, 2025)
        .await
        .unwrap();

    // Assert: Calendar year 2025 total should be Jan: 0.5 + Mar: 1.5 + Sep: 2.5 + Oct: 0.3 + Dec: 0.5 = 5.3
    assert_eq!(
        summary.year_to_date_rainfall_inches, 5.3,
        "Calendar year total should sum monthly rainfall for all months"
    );

    // Cleanup
    sqlx::query!("DELETE FROM monthly_rainfall_summary WHERE station_id = $1", test_station_id)
        .execute(pool)
        .await
        .ok();
    sqlx::query!("DELETE FROM rain_readings WHERE station_id = $1", test_station_id)
        .execute(pool)
        .await
        .ok();
}

#[tokio::test]
async fn test_calendar_year_queries() {
    // Use committed data since service layer needs to see it
    let pool = common::test_pool().await;
    let test_station_id = "TEST_CAL_QUERY_001";

    // Setup: Insert gauge in a transaction
    {
        let mut tx = pool.begin().await.unwrap();
        insert_test_gauge(&mut tx, test_station_id, "Test Calendar Query Gauge").await;
        tx.commit().await.unwrap();
    }

    // Test: Query with service layer
    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo);

    let current_year = Utc::now().year();
    let _summary = reading_service
        .get_calendar_year_summary(test_station_id, current_year)
        .await
        .unwrap();

    // Test passes if query completes without error
}

/// Calculate date range for a specific month (helper for tests)
///
/// Returns (start_of_month, start_of_next_month)
fn month_date_range(year: i32, month: u32) -> (DateTime<Utc>, DateTime<Utc>) {
    let start_date = NaiveDate::from_ymd_opt(year, month, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();

    let (next_year, next_month) = if month == 12 {
        (year + 1, 1)
    } else {
        (year, month + 1)
    };

    let end_date = NaiveDate::from_ymd_opt(next_year, next_month, 1)
        .unwrap()
        .and_hms_opt(0, 0, 0)
        .unwrap();

    let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
    let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

    (start_dt, end_dt)
}

// Integration tests that share a database.
// Each test uses a unique station_id to avoid interference when run concurrently.

use chrono::{DateTime, Datelike, NaiveDate, Utc};
use rain_tracker_service::db::{GaugeRepository, MonthlyRainfallRepository, ReadingRepository};
use rain_tracker_service::fetcher::RainReading;
use rain_tracker_service::fopr::MetaStatsData;
use rain_tracker_service::services::ReadingService;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Test fixture module for setting up common test data
mod test_fixtures {
    use super::*;

    /// Test gauge definitions
    pub const TEST_GAUGES: &[(&str, &str)] = &[
        ("TEST_INSERT_001", "Test Insert Gauge"),
        ("TEST_WATER_YEAR_001", "Test Water Year Gauge"),
        ("TEST_WATER_CALC_001", "Test Water Year Calculation Gauge"),
        ("TEST_CAL_CALC_001", "Test Calendar Calculation Gauge"),
        ("TEST_CAL_QUERY_001", "Test Calendar Query Gauge"),
    ];

    /// Setup test database with fixtures
    pub async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Clean up all test data BEFORE running migrations
        // This prevents foreign key constraint failures during migration
        cleanup_all_test_data(&pool).await;

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Insert test gauges AFTER migrations
        insert_test_gauges(&pool).await;

        pool
    }

    /// Clean up all test data (called before migrations)
    async fn cleanup_all_test_data(pool: &PgPool) {
        // Truncate all tables to ensure clean state
        // This prevents foreign key constraint failures during migration
        // Use TRUNCATE CASCADE to handle foreign keys
        sqlx::query("TRUNCATE TABLE monthly_rainfall_summary, rain_readings, gauge_summaries, gauges CASCADE")
            .execute(pool)
            .await
            .ok();
    }

    /// Insert test gauge fixtures into the database
    async fn insert_test_gauges(pool: &PgPool) {
        let gauge_repo = GaugeRepository::new(pool.clone());

        for (station_id, station_name) in TEST_GAUGES {
            // Check if gauge already exists
            if gauge_repo.gauge_exists(station_id).await.unwrap_or(false) {
                continue;
            }

            // Create minimal test gauge metadata
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
                .upsert_gauge_metadata(&metadata)
                .await
                .expect("Failed to insert test gauge");
        }
    }

    /// Clean up test data for a specific station_id
    pub async fn cleanup_test_data(pool: &PgPool, station_id: &str) {
        // Clean up in correct order due to foreign keys
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
    }
}

#[tokio::test]
async fn test_insert_and_retrieve_readings() {
    // Setup test database with fixtures
    let pool = test_fixtures::setup_test_db().await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo.clone(), monthly_rainfall_repo);

    // Use unique station_id for this test to avoid conflicts with concurrent tests
    let test_station_id = "TEST_INSERT_001";

    // Clean up any existing test data
    test_fixtures::cleanup_test_data(&pool, test_station_id).await;

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

    // Insert readings - need to manually insert with station_id since insert_readings uses default
    for reading in &readings {
        sqlx::query!(
            r#"
            INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
            VALUES ($1, $2, $3, $4)
            "#,
            reading.reading_datetime,
            reading.cumulative_inches,
            reading.incremental_inches,
            test_station_id
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Retrieve latest reading
    let latest = reading_service
        .get_latest_reading(test_station_id)
        .await
        .unwrap();
    assert!(latest.is_some());
}

#[tokio::test]
async fn test_water_year_queries() {
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

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo);

    // Use unique station_id for this test to avoid conflicts with concurrent tests
    let test_station_id = "TEST_WATER_YEAR_001";

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
    use chrono::TimeZone;

    // Setup test database with fixtures
    let pool = test_fixtures::setup_test_db().await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo.clone(), monthly_rainfall_repo.clone());

    // Use unique station_id for this test to avoid conflicts with concurrent tests
    let test_station_id = "TEST_WATER_CALC_001";

    // Clean up any existing test data
    test_fixtures::cleanup_test_data(&pool, test_station_id).await;

    // Create test readings for water year 2024 (Oct 1, 2023 - Sep 30, 2024)
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
            cumulative_inches: 5.75, // This is the final cumulative for the water year
            incremental_inches: 3.45,
        },
    ];

    // Insert readings with custom station_id
    for reading in &readings {
        sqlx::query!(
            r#"
            INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
            VALUES ($1, $2, $3, $4)
            "#,
            reading.reading_datetime,
            reading.cumulative_inches,
            reading.incremental_inches,
            test_station_id
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Populate monthly aggregates
    let (start, end) = month_date_range(2023, 10);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2023, 10, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 3);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2024, 3, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 9);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2024, 9, start, end)
        .await
        .unwrap();

    // Get water year summary for 2024
    let summary = reading_service
        .get_water_year_summary(test_station_id, 2024)
        .await
        .unwrap();

    // total_rainfall_inches should equal the sum of monthly incremental values
    // Oct: 0.5 + Mar: 1.8 + Sep: 3.45 = 5.75
    assert_eq!(
        summary.total_rainfall_inches, 5.75,
        "Total rainfall should equal sum of monthly incremental values for the water year"
    );
    assert_eq!(summary.total_readings, 3);
}

#[tokio::test]
async fn test_calendar_year_total_rainfall_calculation() {
    use chrono::TimeZone;

    // Setup test database with fixtures
    let pool = test_fixtures::setup_test_db().await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo.clone(), monthly_rainfall_repo.clone());

    // Use unique station_id for this test to avoid conflicts with concurrent tests
    let test_station_id = "TEST_CAL_CALC_001";

    // Clean up any existing test data
    test_fixtures::cleanup_test_data(&pool, test_station_id).await;

    // Create test readings for calendar year 2025
    // Calendar year spans two water years:
    // - Jan-Sep 2025: end of water year 2025 (Oct 2024 - Sep 2025)
    // - Oct-Dec 2025: start of water year 2026 (Oct 2025 - Sep 2026)
    let readings = vec![
        // Dec 31, 2024 - baseline (cumulative = 0.5 since Oct 1, 2024)
        // This is Oct-Dec 2024 rainfall (not part of calendar year 2025)
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2024, 12, 31, 12, 0, 0).unwrap(),
            cumulative_inches: 0.5,
            incremental_inches: 0.1,
        },
        // January - cumulative is 1.0 (since Oct 1, 2024)
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2025, 1, 31, 12, 0, 0).unwrap(),
            cumulative_inches: 1.0,
            incremental_inches: 0.5,
        },
        // March - cumulative is 2.5 (since Oct 1, 2024)
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2025, 3, 31, 12, 0, 0).unwrap(),
            cumulative_inches: 2.5,
            incremental_inches: 1.5,
        },
        // September (end of water year) - cumulative is 5.0 (total for water year 2025)
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2025, 9, 30, 12, 0, 0).unwrap(),
            cumulative_inches: 5.0,
            incremental_inches: 2.5,
        },
        // October (start of new water year) - cumulative resets to 0.3
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2025, 10, 31, 12, 0, 0).unwrap(),
            cumulative_inches: 0.3,
            incremental_inches: 0.3,
        },
        // December - cumulative is 0.8 (since Oct 1, 2025)
        RainReading {
            reading_datetime: Utc.with_ymd_and_hms(2025, 12, 31, 12, 0, 0).unwrap(),
            cumulative_inches: 0.8,
            incremental_inches: 0.5,
        },
    ];

    // Insert readings with custom station_id
    for reading in &readings {
        sqlx::query!(
            r#"
            INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
            VALUES ($1, $2, $3, $4)
            "#,
            reading.reading_datetime,
            reading.cumulative_inches,
            reading.incremental_inches,
            test_station_id
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Populate monthly aggregates
    let (start, end) = month_date_range(2024, 12);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2024, 12, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2025, 1);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2025, 1, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2025, 3);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2025, 3, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2025, 9);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2025, 9, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2025, 10);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2025, 10, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2025, 12);
    monthly_rainfall_repo
        .recalculate_monthly_summary(test_station_id, 2025, 12, start, end)
        .await
        .unwrap();

    // Get calendar year summary for 2025
    let summary = reading_service
        .get_calendar_year_summary(test_station_id, 2025)
        .await
        .unwrap();

    // Calendar year 2025 total should be:
    // - Jan: 0.5 + Mar: 1.5 + Sep: 2.5 + Oct: 0.3 + Dec: 0.5 = 5.3
    assert_eq!(
        summary.year_to_date_rainfall_inches, 5.3,
        "Calendar year total should sum monthly rainfall for all months"
    );
}

#[tokio::test]
async fn test_calendar_year_queries() {
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

    let reading_repo = ReadingRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo);

    // Use unique station_id for this test to avoid conflicts with concurrent tests
    let test_station_id = "TEST_CAL_QUERY_001";

    // Query for current calendar year
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

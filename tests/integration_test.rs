use chrono::{Datelike, Utc};
use rain_tracker_service::db::ReadingRepository;
use rain_tracker_service::fetcher::RainReading;
use rain_tracker_service::services::ReadingService;
use sqlx::postgres::PgPoolOptions;

#[tokio::test]
async fn test_insert_and_retrieve_readings() {
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
    let reading_service = ReadingService::new(reading_repo.clone());

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
    let inserted = reading_repo.insert_readings(&readings).await.unwrap();
    assert!(inserted > 0);

    // Retrieve latest reading (using a test station ID)
    let latest = reading_service.get_latest_reading("test_station").await.unwrap();
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
    let reading_service = ReadingService::new(reading_repo);

    // Query for current rain year
    let current_water_year = ReadingService::get_water_year(Utc::now());
    let _summary = reading_service
        .get_water_year_summary("test_station", current_water_year)
        .await
        .unwrap();

    // Test passes if query completes without error
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
    let reading_service = ReadingService::new(reading_repo);

    // Query for current calendar year
    let current_year = Utc::now().year();
    let _summary = reading_service
        .get_calendar_year_summary("test_station", current_year)
        .await
        .unwrap();

    // Test passes if query completes without error
}

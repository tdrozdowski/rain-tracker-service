// API integration tests that verify HTTP endpoints
// Tests actual Axum router with real HTTP requests

use axum::body::Body;
use axum::http::{Request, StatusCode};
use chrono::{TimeZone, Utc};
use http_body_util::BodyExt; // For `.collect()`
use rain_tracker_service::api::{create_router, AppState};
use rain_tracker_service::db::{
    FoprImportJobRepository, GaugeRepository, MonthlyRainfallRepository, ReadingRepository,
};
use rain_tracker_service::fetcher::RainReading;
use rain_tracker_service::fopr::MetaStatsData;
use rain_tracker_service::services::{GaugeService, ReadingService};
use serde_json::Value;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;
use tower::ServiceExt; // For `oneshot`

/// Test fixture module for API tests
mod api_test_fixtures {
    use super::*;
    use chrono::NaiveDate;

    pub const TEST_API_GAUGE: &str = "TEST_API_001";
    pub const TEST_API_GAUGE_NOT_FOUND: &str = "TEST_API_999"; // For negative tests

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

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Insert test gauge
        insert_test_gauge(&pool).await;

        pool
    }

    /// Insert test gauge for API tests
    async fn insert_test_gauge(pool: &PgPool) {
        let gauge_repo = GaugeRepository::new(pool.clone());

        // Check if gauge already exists in gauge_summaries
        let exists = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM gauge_summaries WHERE station_id = $1"#,
            TEST_API_GAUGE
        )
        .fetch_one(pool)
        .await
        .unwrap_or(Some(0))
        .unwrap_or(0);

        if exists > 0 {
            return;
        }

        // Create test gauge metadata for gauges table
        let metadata = MetaStatsData {
            station_id: TEST_API_GAUGE.to_string(),
            station_name: "Test API Gauge".to_string(),
            previous_station_ids: vec![],
            station_type: "Rain".to_string(),
            latitude: 33.5,
            longitude: -112.0,
            elevation_ft: Some(1000),
            county: "Maricopa".to_string(),
            city: Some("Phoenix".to_string()),
            location_description: Some("Test location for API tests".to_string()),
            installation_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            data_begins_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            status: "Active".to_string(),
            avg_annual_precipitation_inches: Some(8.0),
            complete_years_count: Some(5),
            incomplete_months_count: 0,
            missing_months_count: 0,
            data_quality_remarks: Some("Test gauge for API integration tests".to_string()),
            fopr_metadata: serde_json::Map::new(),
        };

        gauge_repo
            .upsert_gauge_metadata(&metadata)
            .await
            .expect("Failed to insert test gauge");

        // Also insert into gauge_summaries table (which is what the API queries)
        sqlx::query!(
            r#"
            INSERT INTO gauge_summaries (
                station_id, gauge_name, city_town, elevation_ft,
                general_location, msp_forecast_zone,
                rainfall_past_6h_inches, rainfall_past_24h_inches,
                last_scraped_at, updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
            ON CONFLICT (station_id) DO NOTHING
            "#,
            TEST_API_GAUGE,
            "Test API Gauge",
            "Phoenix",
            Some(1000_i32),
            Some("Test location for API tests"),
            Some("MSP01"),
            Some(0.0_f64),
            Some(0.0_f64)
        )
        .execute(pool)
        .await
        .ok(); // Ignore errors from duplicate key
    }

    /// Clean up test data for API tests
    pub async fn cleanup_test_data(pool: &PgPool) {
        sqlx::query!(
            "DELETE FROM monthly_rainfall_summary WHERE station_id = $1",
            TEST_API_GAUGE
        )
        .execute(pool)
        .await
        .ok();

        sqlx::query!(
            "DELETE FROM rain_readings WHERE station_id = $1",
            TEST_API_GAUGE
        )
        .execute(pool)
        .await
        .ok();
    }
}

/// Helper to create test app with real database
async fn create_test_app() -> (axum::Router, PgPool) {
    let pool = api_test_fixtures::setup_test_db().await;

    let reading_repo = ReadingRepository::new(pool.clone());
    let gauge_repo = GaugeRepository::new(pool.clone());
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let job_repo = FoprImportJobRepository::new(pool.clone());

    let reading_service = ReadingService::new(reading_repo, monthly_rainfall_repo);
    let gauge_service = GaugeService::new(gauge_repo, job_repo);

    let state = AppState {
        reading_service,
        gauge_service,
    };

    let router = create_router(state);

    (router, pool)
}

#[tokio::test]
async fn test_health_endpoint() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["status"], "healthy");
}

#[tokio::test]
async fn test_get_latest_reading_not_found() {
    let (app, _pool) = create_test_app().await;

    // Use a different gauge ID that doesn't have any readings
    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/readings/{}/latest",
                    api_test_fixtures::TEST_API_GAUGE_NOT_FOUND
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_latest_reading_success() {
    let (app, pool) = create_test_app().await;
    api_test_fixtures::cleanup_test_data(&pool).await;

    // Insert a test reading
    let test_reading = RainReading {
        reading_datetime: Utc::now(),
        cumulative_inches: 2.5,
        incremental_inches: 0.1,
    };

    sqlx::query!(
        r#"
        INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
        VALUES ($1, $2, $3, $4)
        "#,
        test_reading.reading_datetime,
        test_reading.cumulative_inches,
        test_reading.incremental_inches,
        api_test_fixtures::TEST_API_GAUGE
    )
    .execute(&pool)
    .await
    .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/readings/{}/latest",
                    api_test_fixtures::TEST_API_GAUGE
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["cumulative_inches"], 2.5);
    assert_eq!(json["incremental_inches"], 0.1);
    assert_eq!(json["station_id"], api_test_fixtures::TEST_API_GAUGE);
}

#[tokio::test]
async fn test_water_year_endpoint() {
    let (app, pool) = create_test_app().await;
    api_test_fixtures::cleanup_test_data(&pool).await;

    // Insert test readings for water year 2024 (Oct 2023 - Sep 2024)
    let readings = vec![
        (
            Utc.with_ymd_and_hms(2023, 10, 15, 12, 0, 0).unwrap(),
            0.5,
            0.5,
        ),
        (
            Utc.with_ymd_and_hms(2024, 3, 15, 12, 0, 0).unwrap(),
            2.3,
            1.8,
        ),
        (
            Utc.with_ymd_and_hms(2024, 9, 15, 12, 0, 0).unwrap(),
            5.0,
            2.7,
        ),
    ];

    for (datetime, cumulative, incremental) in readings {
        sqlx::query!(
            r#"
            INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
            VALUES ($1, $2, $3, $4)
            "#,
            datetime,
            cumulative,
            incremental,
            api_test_fixtures::TEST_API_GAUGE
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Populate monthly aggregates
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let (start, end) = month_date_range(2023, 10);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2023, 10, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 3);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2024, 3, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 9);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2024, 9, start, end)
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/readings/{}/water-year/2024",
                    api_test_fixtures::TEST_API_GAUGE
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["water_year"], 2024);
    assert_eq!(json["total_readings"], 3);
    assert_eq!(json["total_rainfall_inches"], 5.0);
    assert!(json["readings"].is_array());
}

/// Calculate date range for a specific month (helper for tests)
fn month_date_range(year: i32, month: u32) -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
    use chrono::NaiveDate;

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

    let start_dt = chrono::DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
    let end_dt = chrono::DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

    (start_dt, end_dt)
}

#[tokio::test]
async fn test_calendar_year_endpoint() {
    let (app, pool) = create_test_app().await;
    api_test_fixtures::cleanup_test_data(&pool).await;

    // Insert test readings for calendar year 2024
    let readings = vec![
        (
            Utc.with_ymd_and_hms(2024, 1, 15, 12, 0, 0).unwrap(),
            0.5,
            0.5,
        ),
        (
            Utc.with_ymd_and_hms(2024, 6, 15, 12, 0, 0).unwrap(),
            2.0,
            1.5,
        ),
        (
            Utc.with_ymd_and_hms(2024, 12, 15, 12, 0, 0).unwrap(),
            4.5,
            2.5,
        ),
    ];

    for (datetime, cumulative, incremental) in readings {
        sqlx::query!(
            r#"
            INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches, station_id)
            VALUES ($1, $2, $3, $4)
            "#,
            datetime,
            cumulative,
            incremental,
            api_test_fixtures::TEST_API_GAUGE
        )
        .execute(&pool)
        .await
        .unwrap();
    }

    // Populate monthly aggregates
    let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
    let (start, end) = month_date_range(2024, 1);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2024, 1, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 6);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2024, 6, start, end)
        .await
        .unwrap();
    let (start, end) = month_date_range(2024, 12);
    monthly_rainfall_repo
        .recalculate_monthly_summary(api_test_fixtures::TEST_API_GAUGE, 2024, 12, start, end)
        .await
        .unwrap();

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/readings/{}/calendar-year/2024",
                    api_test_fixtures::TEST_API_GAUGE
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["calendar_year"], 2024);
    assert_eq!(json["total_readings"], 3);
    assert_eq!(json["year_to_date_rainfall_inches"], 4.5);
    assert!(json["monthly_summaries"].is_array());
    assert!(json["readings"].is_array());
}

#[tokio::test]
async fn test_get_gauge_by_id() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri(format!(
                    "/api/v1/gauges/{}",
                    api_test_fixtures::TEST_API_GAUGE
                ))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["station_id"], api_test_fixtures::TEST_API_GAUGE);
    assert_eq!(json["gauge_name"], "Test API Gauge");
    assert_eq!(json["city_town"], "Phoenix");
}

#[tokio::test]
async fn test_get_gauge_by_id_not_found() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/gauges/NONEXISTENT_GAUGE")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn test_get_all_gauges_default_pagination() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/gauges")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert!(json["gauges"].is_array());
    assert!(json["page"].is_number());
    assert!(json["page_size"].is_number());
    assert!(json["total_pages"].is_number());
    assert!(json["total_gauges"].is_number());
}

#[tokio::test]
async fn test_get_all_gauges_custom_pagination() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/gauges?page=1&page_size=5")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    assert_eq!(json["page"], 1);
    assert_eq!(json["page_size"], 5);
    assert!(json["gauges"].as_array().unwrap().len() <= 5);
}

#[tokio::test]
async fn test_openapi_spec_endpoint() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(
            Request::builder()
                .uri("/api-docs/openapi.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let json: Value = serde_json::from_slice(&body).unwrap();

    // Verify OpenAPI structure
    assert!(json["openapi"].is_string());
    assert!(json["info"].is_object());
    assert_eq!(json["info"]["title"], "Rain Tracker Service API");
    assert!(json["paths"].is_object());
}

#[tokio::test]
async fn test_redoc_ui_endpoint() {
    let (app, _pool) = create_test_app().await;

    let response = app
        .oneshot(Request::builder().uri("/docs").body(Body::empty()).unwrap())
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);

    let body = response.into_body().collect().await.unwrap().to_bytes();
    let html = String::from_utf8(body.to_vec()).unwrap();

    assert!(html.contains("<title>Rain Tracker API Documentation</title>"));
    assert!(html.contains("redoc"));
}

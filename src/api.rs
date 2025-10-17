use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    routing::get,
};
use serde::Serialize;
use tracing::{debug, error, info, instrument, warn};

use crate::db::Reading;
use crate::services::ReadingService;

#[derive(Clone)]
pub struct AppState {
    pub reading_service: ReadingService,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub latest_reading: Option<Reading>,
}

pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/readings/water-year/{year}", get(get_water_year))
        .route("/readings/calendar-year/{year}", get(get_calendar_year))
        .route("/readings/latest", get(get_latest))
        .with_state(state);

    Router::new().nest("/api/v1", api_routes)
}

#[instrument(skip(state))]
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    debug!("Health check requested");
    match state.reading_service.get_latest_reading().await {
        Ok(latest) => {
            info!("Health check successful, latest reading present: {}", latest.is_some());
            let response = HealthResponse {
                status: "healthy".to_string(),
                latest_reading: latest,
            };
            (StatusCode::OK, Json(response))
        }
        Err(e) => {
            error!("Health check failed: {}", e);
            let response = HealthResponse {
                status: format!("unhealthy: {}", e),
                latest_reading: None,
            };
            (StatusCode::SERVICE_UNAVAILABLE, Json(response))
        }
    }
}

#[instrument(skip(state), fields(year = %year))]
async fn get_water_year(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Result<Json<crate::db::WaterYearSummary>, StatusCode> {
    debug!("Fetching rain year readings for year {}", year);
    let summary = state
        .reading_service
        .get_water_year_summary(year)
        .await
        .map_err(|e| {
            error!("Failed to fetch rain year readings for {}: {}", year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} readings for rain year {}, total rainfall: {:.2} inches",
        summary.total_readings,
        year,
        summary.total_rainfall_inches
    );

    Ok(Json(summary))
}

#[instrument(skip(state), fields(year = %year))]
async fn get_calendar_year(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Result<Json<crate::db::CalendarYearSummary>, StatusCode> {
    debug!("Fetching calendar year readings for year {}", year);
    let summary = state
        .reading_service
        .get_calendar_year_summary(year)
        .await
        .map_err(|e| {
            error!("Failed to fetch calendar year readings for {}: {}", year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} readings for calendar year {}, YTD rainfall: {:.2} inches",
        summary.total_readings,
        year,
        summary.year_to_date_rainfall_inches
    );

    Ok(Json(summary))
}

#[instrument(skip(state))]
async fn get_latest(State(state): State<AppState>) -> Result<Json<Reading>, StatusCode> {
    debug!("Fetching latest reading");
    let reading = state
        .reading_service
        .get_latest_reading()
        .await
        .map_err(|e| {
            error!("Failed to fetch latest reading: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("No readings found in database");
            StatusCode::NOT_FOUND
        })?;

    info!(
        "Retrieved latest reading from {}",
        reading.reading_datetime
    );

    Ok(Json(reading))
}

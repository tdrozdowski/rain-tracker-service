use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::IntoResponse,
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tracing::{debug, error, info, instrument, warn};

use crate::db::Reading;
use crate::services::gauge_service::PaginationParams;
use crate::services::{GaugeService, ReadingService};

#[derive(Clone)]
pub struct AppState {
    pub reading_service: ReadingService,
    pub gauge_service: GaugeService,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
}

pub fn create_router(state: AppState) -> Router {
    let api_routes = Router::new()
        .route("/health", get(health))
        .route("/readings/{station_id}/water-year/{year}", get(get_water_year))
        .route("/readings/{station_id}/calendar-year/{year}", get(get_calendar_year))
        .route("/readings/{station_id}/latest", get(get_latest))
        .route("/gauges", get(get_all_gauges))
        .route("/gauges/{station_id}", get(get_gauge_by_id))
        .with_state(state);

    Router::new().nest("/api/v1", api_routes)
}

#[instrument(skip(_state))]
async fn health(State(_state): State<AppState>) -> impl IntoResponse {
    debug!("Health check requested");
    info!("Health check successful");
    let response = HealthResponse {
        status: "healthy".to_string(),
    };
    (StatusCode::OK, Json(response))
}

#[instrument(skip(state), fields(station_id = %station_id, year = %year))]
async fn get_water_year(
    State(state): State<AppState>,
    Path((station_id, year)): Path<(String, i32)>,
) -> Result<Json<crate::db::WaterYearSummary>, StatusCode> {
    debug!("Fetching rain year readings for gauge {} year {}", station_id, year);
    let summary = state
        .reading_service
        .get_water_year_summary(&station_id, year)
        .await
        .map_err(|e| {
            error!("Failed to fetch rain year readings for gauge {} year {}: {}", station_id, year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} readings for gauge {} rain year {}, total rainfall: {:.2} inches",
        summary.total_readings, station_id, year, summary.total_rainfall_inches
    );

    Ok(Json(summary))
}

#[instrument(skip(state), fields(station_id = %station_id, year = %year))]
async fn get_calendar_year(
    State(state): State<AppState>,
    Path((station_id, year)): Path<(String, i32)>,
) -> Result<Json<crate::db::CalendarYearSummary>, StatusCode> {
    debug!("Fetching calendar year readings for gauge {} year {}", station_id, year);
    let summary = state
        .reading_service
        .get_calendar_year_summary(&station_id, year)
        .await
        .map_err(|e| {
            error!("Failed to fetch calendar year readings for gauge {} year {}: {}", station_id, year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} readings for gauge {} calendar year {}, YTD rainfall: {:.2} inches",
        summary.total_readings, station_id, year, summary.year_to_date_rainfall_inches
    );

    Ok(Json(summary))
}

#[instrument(skip(state), fields(station_id = %station_id))]
async fn get_latest(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
) -> Result<Json<Reading>, StatusCode> {
    debug!("Fetching latest reading for gauge {}", station_id);
    let reading = state
        .reading_service
        .get_latest_reading(&station_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch latest reading for gauge {}: {}", station_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("No readings found for gauge {}", station_id);
            StatusCode::NOT_FOUND
        })?;

    info!("Retrieved latest reading for gauge {} from {}", station_id, reading.reading_datetime);

    Ok(Json(reading))
}

#[instrument(skip(state))]
async fn get_all_gauges(
    State(state): State<AppState>,
    Query(params): Query<PaginationParams>,
) -> Result<Json<crate::services::gauge_service::GaugeListResponse>, StatusCode> {
    debug!(
        "Fetching gauge summaries (page={}, page_size={})",
        params.page, params.page_size
    );

    let response = state
        .gauge_service
        .get_gauges_paginated(&params)
        .await
        .map_err(|e| {
            error!("Failed to fetch gauges: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    info!(
        "Retrieved {} gauge summaries (page {}/{}, total={})",
        response.gauges.len(),
        response.page,
        response.total_pages,
        response.total_gauges
    );

    Ok(Json(response))
}

#[instrument(skip(state), fields(station_id = %station_id))]
async fn get_gauge_by_id(
    State(state): State<AppState>,
    Path(station_id): Path<String>,
) -> Result<Json<crate::db::GaugeSummary>, StatusCode> {
    debug!("Fetching gauge summary for station {}", station_id);

    let gauge = state
        .gauge_service
        .get_gauge_by_id(&station_id)
        .await
        .map_err(|e| {
            error!("Failed to fetch gauge {}: {}", station_id, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?
        .ok_or_else(|| {
            warn!("Gauge {} not found", station_id);
            StatusCode::NOT_FOUND
        })?;

    info!("Retrieved gauge summary for station {}", station_id);
    Ok(Json(gauge))
}

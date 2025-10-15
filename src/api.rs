use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::IntoResponse,
    Json, Router,
    routing::get,
};
use serde::Serialize;
use std::sync::Arc;
use tracing::{debug, error, info, instrument, warn};

use crate::db::{RainDb, StoredReading};

#[derive(Clone)]
pub struct AppState {
    pub db: Arc<RainDb>,
}

#[derive(Serialize)]
pub struct WaterYearSummary {
    pub water_year: i32,
    pub total_readings: usize,
    pub total_rainfall_inches: f64,
    pub readings: Vec<StoredReading>,
}

#[derive(Serialize)]
pub struct MonthlySummary {
    pub month: u32,
    pub month_name: String,
    pub readings_count: usize,
    pub month_rainfall_inches: f64,
    pub cumulative_ytd_inches: f64,
}

#[derive(Serialize)]
pub struct CalendarYearSummary {
    pub calendar_year: i32,
    pub total_readings: usize,
    pub year_to_date_rainfall_inches: f64,
    pub monthly_summaries: Vec<MonthlySummary>,
    pub readings: Vec<StoredReading>,
}

#[derive(Serialize)]
pub struct HealthResponse {
    pub status: String,
    pub latest_reading: Option<StoredReading>,
}

pub fn create_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/readings/water-year/{year}", get(get_water_year))
        .route("/readings/calendar-year/{year}", get(get_calendar_year))
        .route("/readings/latest", get(get_latest))
        .with_state(state)
}

#[instrument(skip(state))]
async fn health(State(state): State<AppState>) -> impl IntoResponse {
    debug!("Health check requested");
    match state.db.get_latest_reading().await {
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
) -> Result<Json<WaterYearSummary>, StatusCode> {
    debug!("Fetching rain year readings for year {}", year);
    let readings = state
        .db
        .get_water_year_readings(year)
        .await
        .map_err(|e| {
            error!("Failed to fetch rain year readings for {}: {}", year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let total_rainfall = calculate_total_rainfall(&readings);

    info!(
        "Retrieved {} readings for rain year {}, total rainfall: {:.2} inches",
        readings.len(),
        year,
        total_rainfall
    );

    Ok(Json(WaterYearSummary {
        water_year: year,
        total_readings: readings.len(),
        total_rainfall_inches: total_rainfall,
        readings,
    }))
}

#[instrument(skip(state), fields(year = %year))]
async fn get_calendar_year(
    State(state): State<AppState>,
    Path(year): Path<i32>,
) -> Result<Json<CalendarYearSummary>, StatusCode> {
    debug!("Fetching calendar year readings for year {}", year);
    let mut readings = state
        .db
        .get_calendar_year_readings(year)
        .await
        .map_err(|e| {
            error!("Failed to fetch calendar year readings for {}: {}", year, e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    // Sort readings by datetime ascending for proper month grouping
    readings.sort_by_key(|r| r.reading_datetime);

    let monthly_summaries = calculate_monthly_summaries(&readings);

    // YTD rainfall should be the cumulative value from the last month that has readings
    // This accounts for the water year boundary (Oct-Dec include previous water year)
    let year_to_date_rainfall = monthly_summaries
        .iter()
        .rev()
        .find(|m| m.readings_count > 0)
        .map(|m| m.cumulative_ytd_inches)
        .unwrap_or(0.0);

    info!(
        "Retrieved {} readings for calendar year {}, YTD rainfall: {:.2} inches",
        readings.len(),
        year,
        year_to_date_rainfall
    );

    // Reverse readings back to descending order for API response
    readings.reverse();

    Ok(Json(CalendarYearSummary {
        calendar_year: year,
        total_readings: readings.len(),
        year_to_date_rainfall_inches: year_to_date_rainfall,
        monthly_summaries,
        readings,
    }))
}

#[instrument(skip(state))]
async fn get_latest(State(state): State<AppState>) -> Result<Json<StoredReading>, StatusCode> {
    debug!("Fetching latest reading");
    let reading = state
        .db
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

fn calculate_total_rainfall(readings: &[StoredReading]) -> f64 {
    readings.iter().map(|r| r.incremental_inches).sum()
}

fn calculate_monthly_summaries(readings: &[StoredReading]) -> Vec<MonthlySummary> {
    use chrono::Datelike;
    use std::collections::HashMap;

    // Group readings by month
    let mut monthly_data: HashMap<u32, Vec<&StoredReading>> = HashMap::new();
    for reading in readings {
        let month = reading.reading_datetime.month();
        monthly_data.entry(month).or_insert_with(Vec::new).push(reading);
    }

    // Find the last reading in September (end of previous water year) to get baseline for Oct-Dec
    let sept_final_cumulative = if let Some(sept_readings) = monthly_data.get(&9) {
        // Get the latest reading in September
        sept_readings.iter()
            .max_by_key(|r| r.reading_datetime)
            .map(|r| r.cumulative_inches)
            .unwrap_or(0.0)
    } else {
        0.0
    };

    // Calculate monthly summaries with cumulative values
    let mut summaries = Vec::new();
    let mut cumulative_jan_sept = 0.0;  // Accumulator for Jan-Sept (water year portion in calendar year)
    let mut cumulative_oct_dec = 0.0;   // Accumulator for Oct-Dec (new water year)

    for month in 1..=12 {
        if let Some(month_readings) = monthly_data.get(&month) {
            let month_rainfall: f64 = month_readings.iter().map(|r| r.incremental_inches).sum();

            let cumulative_ytd = if month >= 10 {
                // Oct-Dec: add previous water year total (Sept final) + new water year accumulation
                cumulative_oct_dec += month_rainfall;
                sept_final_cumulative + cumulative_oct_dec
            } else {
                // Jan-Sept: normal accumulation within current water year
                cumulative_jan_sept += month_rainfall;
                cumulative_jan_sept
            };

            summaries.push(MonthlySummary {
                month,
                month_name: get_month_name(month),
                readings_count: month_readings.len(),
                month_rainfall_inches: month_rainfall,
                cumulative_ytd_inches: cumulative_ytd,
            });
        } else {
            // Month with no readings - still show it with zeros but maintain cumulative
            let cumulative_ytd = if month >= 10 {
                sept_final_cumulative + cumulative_oct_dec
            } else {
                cumulative_jan_sept
            };

            summaries.push(MonthlySummary {
                month,
                month_name: get_month_name(month),
                readings_count: 0,
                month_rainfall_inches: 0.0,
                cumulative_ytd_inches: cumulative_ytd,
            });
        }
    }

    summaries
}

fn get_month_name(month: u32) -> String {
    match month {
        1 => "January",
        2 => "February",
        3 => "March",
        4 => "April",
        5 => "May",
        6 => "June",
        7 => "July",
        8 => "August",
        9 => "September",
        10 => "October",
        11 => "November",
        12 => "December",
        _ => "Unknown",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Utc};

    fn create_test_reading(year: i32, month: u32, day: u32, cumulative: f64, incremental: f64) -> StoredReading {
        StoredReading {
            id: 1,
            reading_datetime: Utc.with_ymd_and_hms(year, month, day, 12, 0, 0).unwrap(),
            cumulative_inches: cumulative,
            incremental_inches: incremental,
            station_id: "TEST".to_string(),
            created_at: Utc::now(),
        }
    }

    #[test]
    fn test_monthly_summaries_water_year_boundary() {
        // Test case: Calendar year with readings spanning water year boundary
        // Sept has cumulative 10.0 inches (end of water year 2024)
        // Oct-Dec are in new water year 2025, should add to Sept's total
        let readings = vec![
            // January - March (water year 2024)
            create_test_reading(2024, 1, 15, 2.0, 2.0),
            create_test_reading(2024, 2, 15, 4.0, 2.0),
            create_test_reading(2024, 3, 15, 6.0, 2.0),
            // September - end of water year 2024
            create_test_reading(2024, 9, 30, 10.0, 4.0),
            // October - December (water year 2025)
            create_test_reading(2024, 10, 15, 1.5, 1.5),
            create_test_reading(2024, 11, 15, 3.0, 1.5),
            create_test_reading(2024, 12, 15, 4.5, 1.5),
        ];

        let summaries = calculate_monthly_summaries(&readings);

        // Find specific months
        let jan = summaries.iter().find(|s| s.month == 1).unwrap();
        let feb = summaries.iter().find(|s| s.month == 2).unwrap();
        let mar = summaries.iter().find(|s| s.month == 3).unwrap();
        let sept = summaries.iter().find(|s| s.month == 9).unwrap();
        let oct = summaries.iter().find(|s| s.month == 10).unwrap();
        let nov = summaries.iter().find(|s| s.month == 11).unwrap();
        let dec = summaries.iter().find(|s| s.month == 12).unwrap();

        // Jan-Sept should accumulate normally
        assert_eq!(jan.cumulative_ytd_inches, 2.0);
        assert_eq!(feb.cumulative_ytd_inches, 4.0);
        assert_eq!(mar.cumulative_ytd_inches, 6.0);
        assert_eq!(sept.cumulative_ytd_inches, 10.0);

        // Oct-Dec should be: Sept's final (10.0) + new water year accumulation
        assert_eq!(oct.cumulative_ytd_inches, 10.0 + 1.5);
        assert_eq!(nov.cumulative_ytd_inches, 10.0 + 3.0);
        assert_eq!(dec.cumulative_ytd_inches, 10.0 + 4.5);
    }

    #[test]
    fn test_monthly_summaries_no_september_readings() {
        // Test case: No September readings (edge case)
        let readings = vec![
            create_test_reading(2024, 1, 15, 2.0, 2.0),
            create_test_reading(2024, 10, 15, 1.5, 1.5),
            create_test_reading(2024, 11, 15, 3.0, 1.5),
        ];

        let summaries = calculate_monthly_summaries(&readings);

        let jan = summaries.iter().find(|s| s.month == 1).unwrap();
        let oct = summaries.iter().find(|s| s.month == 10).unwrap();
        let nov = summaries.iter().find(|s| s.month == 11).unwrap();

        // Jan should accumulate normally
        assert_eq!(jan.cumulative_ytd_inches, 2.0);

        // Oct-Dec should start from 0 (no Sept baseline) + new accumulation
        assert_eq!(oct.cumulative_ytd_inches, 1.5);
        assert_eq!(nov.cumulative_ytd_inches, 3.0);
    }

    #[test]
    fn test_monthly_summaries_only_oct_dec_readings() {
        // Test case: Only Oct-Dec readings in calendar year
        let readings = vec![
            create_test_reading(2024, 10, 15, 1.0, 1.0),
            create_test_reading(2024, 11, 15, 2.5, 1.5),
            create_test_reading(2024, 12, 15, 4.0, 1.5),
        ];

        let summaries = calculate_monthly_summaries(&readings);

        let oct = summaries.iter().find(|s| s.month == 10).unwrap();
        let nov = summaries.iter().find(|s| s.month == 11).unwrap();
        let dec = summaries.iter().find(|s| s.month == 12).unwrap();

        // No Sept baseline, so just accumulate from Oct
        assert_eq!(oct.cumulative_ytd_inches, 1.0);
        assert_eq!(nov.cumulative_ytd_inches, 2.5);
        assert_eq!(dec.cumulative_ytd_inches, 4.0);
    }

    #[test]
    fn test_monthly_summaries_months_with_no_readings() {
        // Test case: Gaps in months
        let readings = vec![
            create_test_reading(2024, 1, 15, 2.0, 2.0),
            // No Feb readings
            create_test_reading(2024, 3, 15, 4.0, 2.0),
            create_test_reading(2024, 9, 30, 10.0, 6.0),
            // No Oct readings
            create_test_reading(2024, 11, 15, 1.5, 1.5),
        ];

        let summaries = calculate_monthly_summaries(&readings);

        let jan = summaries.iter().find(|s| s.month == 1).unwrap();
        let feb = summaries.iter().find(|s| s.month == 2).unwrap();
        let mar = summaries.iter().find(|s| s.month == 3).unwrap();
        let sept = summaries.iter().find(|s| s.month == 9).unwrap();
        let oct = summaries.iter().find(|s| s.month == 10).unwrap();
        let nov = summaries.iter().find(|s| s.month == 11).unwrap();

        // Feb has no readings but should maintain cumulative from Jan
        assert_eq!(jan.cumulative_ytd_inches, 2.0);
        assert_eq!(feb.cumulative_ytd_inches, 2.0);
        assert_eq!(feb.readings_count, 0);
        assert_eq!(mar.cumulative_ytd_inches, 4.0);
        assert_eq!(sept.cumulative_ytd_inches, 10.0);

        // Oct has no readings but should have Sept baseline
        assert_eq!(oct.cumulative_ytd_inches, 10.0);
        assert_eq!(oct.readings_count, 0);

        // Nov should be Sept baseline + its rainfall
        assert_eq!(nov.cumulative_ytd_inches, 10.0 + 1.5);
    }

    #[test]
    fn test_monthly_summaries_multiple_readings_per_month() {
        // Test case: Multiple readings in September, should use the latest one's cumulative
        let readings = vec![
            create_test_reading(2024, 9, 10, 8.0, 1.0),
            create_test_reading(2024, 9, 20, 9.0, 1.0),
            create_test_reading(2024, 9, 30, 10.0, 1.0),
            create_test_reading(2024, 10, 15, 1.5, 1.5),
        ];

        let summaries = calculate_monthly_summaries(&readings);

        let sept = summaries.iter().find(|s| s.month == 9).unwrap();
        let oct = summaries.iter().find(|s| s.month == 10).unwrap();

        // September should sum all its incremental values
        assert_eq!(sept.month_rainfall_inches, 3.0);
        assert_eq!(sept.cumulative_ytd_inches, 3.0);

        // October should use the LATEST September reading's cumulative (10.0)
        assert_eq!(oct.cumulative_ytd_inches, 10.0 + 1.5);
    }
}

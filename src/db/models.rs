use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::FromRow;

// Database entity models
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct Reading {
    pub id: i64,
    pub reading_datetime: DateTime<Utc>,
    pub cumulative_inches: f64,
    pub incremental_inches: f64,
    pub station_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct GaugeSummary {
    pub id: i64,
    pub station_id: String,
    pub gauge_name: String,
    pub city_town: Option<String>,
    pub elevation_ft: Option<i32>,
    pub general_location: Option<String>,
    pub msp_forecast_zone: Option<String>,
    pub rainfall_past_6h_inches: Option<f64>,
    pub rainfall_past_24h_inches: Option<f64>,
    pub last_scraped_at: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// API response DTOs (to avoid circular dependency between services and api modules)
#[derive(Debug, Clone, Serialize)]
pub struct WaterYearSummary {
    pub water_year: i32,
    pub total_readings: usize,
    pub total_rainfall_inches: f64,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CalendarYearSummary {
    pub calendar_year: i32,
    pub total_readings: usize,
    pub year_to_date_rainfall_inches: f64,
    pub monthly_summaries: Vec<MonthlySummary>,
    pub readings: Vec<Reading>,
}

#[derive(Debug, Clone, Serialize)]
pub struct MonthlySummary {
    pub month: u32,
    pub month_name: String,
    pub readings_count: usize,
    pub monthly_rainfall_inches: f64,
    pub cumulative_ytd_inches: f64,
}

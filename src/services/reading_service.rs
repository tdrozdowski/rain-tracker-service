use chrono::{DateTime, Datelike, NaiveDate, Utc};
use std::collections::HashMap;

use crate::db::{
    CalendarYearSummary, DbError, MonthlyRainfallRepository, MonthlySummary, Reading,
    ReadingRepository, WaterYearSummary,
};

#[derive(Clone)]
pub struct ReadingService {
    reading_repo: ReadingRepository,
    monthly_rainfall_repo: MonthlyRainfallRepository,
}

impl ReadingService {
    pub fn new(
        reading_repo: ReadingRepository,
        monthly_rainfall_repo: MonthlyRainfallRepository,
    ) -> Self {
        Self {
            reading_repo,
            monthly_rainfall_repo,
        }
    }

    /// Get water year summary with business logic
    pub async fn get_water_year_summary(
        &self,
        station_id: &str,
        water_year: i32,
    ) -> Result<WaterYearSummary, DbError> {
        // Fetch monthly summaries for the water year (Oct prev year - Sep current year)
        let monthly_summaries = self
            .monthly_rainfall_repo
            .get_water_year_summaries(station_id, water_year)
            .await?;

        // Calculate total rainfall by summing monthly totals
        let total_rainfall: f64 = monthly_summaries
            .iter()
            .map(|m| m.total_rainfall_inches)
            .sum();

        // Calculate total readings count
        let total_readings: i32 = monthly_summaries.iter().map(|m| m.reading_count).sum();

        // Fetch actual readings for detailed view
        let (start, end) = Self::water_year_date_range(water_year);
        let readings = self
            .reading_repo
            .find_by_date_range(station_id, start, end)
            .await?;

        Ok(WaterYearSummary {
            water_year,
            total_readings: total_readings as usize,
            total_rainfall_inches: Self::normalize_zero(total_rainfall),
            readings,
        })
    }

    /// Get calendar year summary with monthly breakdowns
    pub async fn get_calendar_year_summary(
        &self,
        station_id: &str,
        year: i32,
    ) -> Result<CalendarYearSummary, DbError> {
        // Fetch monthly summaries for the calendar year
        let monthly_summaries_db = self
            .monthly_rainfall_repo
            .get_calendar_year_summaries(station_id, year)
            .await?;

        // Calculate year-to-date rainfall by summing monthly totals
        let year_to_date_rainfall: f64 = monthly_summaries_db
            .iter()
            .map(|m| m.total_rainfall_inches)
            .sum();

        // Fetch actual readings for detailed view (calendar year only)
        let (start, end) = Self::calendar_year_date_range_only(year);
        let mut readings = self
            .reading_repo
            .find_by_date_range(station_id, start, end)
            .await?;

        // Convert database monthly summaries to API format with cumulative YTD
        let monthly_summaries = Self::build_monthly_summaries(&monthly_summaries_db);

        readings.reverse(); // Desc for API

        Ok(CalendarYearSummary {
            calendar_year: year,
            total_readings: readings.len(),
            year_to_date_rainfall_inches: Self::normalize_zero(year_to_date_rainfall),
            monthly_summaries,
            readings,
        })
    }

    /// Get latest reading for a specific gauge
    pub async fn get_latest_reading(&self, station_id: &str) -> Result<Option<Reading>, DbError> {
        self.reading_repo.find_latest(station_id).await
    }

    // Business logic helpers (private)

    /// Normalize -0.0 to 0.0 for cleaner API responses
    fn normalize_zero(value: f64) -> f64 {
        if value == 0.0 {
            0.0 // Converts both 0.0 and -0.0 to 0.0
        } else {
            value
        }
    }

    fn water_year_date_range(water_year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_date = NaiveDate::from_ymd_opt(water_year - 1, 10, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(water_year, 10, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
        let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

        (start_dt, end_dt)
    }

    fn calendar_year_date_range_only(year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
        let start_date = NaiveDate::from_ymd_opt(year, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let end_date = NaiveDate::from_ymd_opt(year + 1, 1, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();

        let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);
        let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

        (start_dt, end_dt)
    }

    fn build_monthly_summaries(
        monthly_summaries_db: &[crate::db::MonthlyRainfallSummary],
    ) -> Vec<MonthlySummary> {
        let mut summaries = Vec::new();
        let mut cumulative_ytd = 0.0;

        // Create a map for quick lookup
        let db_map: HashMap<i32, &crate::db::MonthlyRainfallSummary> =
            monthly_summaries_db.iter().map(|s| (s.month, s)).collect();

        for month in 1..=12 {
            if let Some(db_summary) = db_map.get(&month) {
                cumulative_ytd += db_summary.total_rainfall_inches;
                summaries.push(MonthlySummary {
                    month: month as u32,
                    month_name: Self::get_month_name(month as u32),
                    readings_count: db_summary.reading_count as usize,
                    monthly_rainfall_inches: db_summary.total_rainfall_inches,
                    cumulative_ytd_inches: cumulative_ytd,
                });
            } else {
                // Month with no data - rainfall is 0, but maintain cumulative
                summaries.push(MonthlySummary {
                    month: month as u32,
                    month_name: Self::get_month_name(month as u32),
                    readings_count: 0,
                    monthly_rainfall_inches: 0.0,
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

    /// Determine which water year a date falls into
    pub fn get_water_year(date: DateTime<Utc>) -> i32 {
        let year = date.year();
        let month = date.month();

        if month >= 10 {
            year + 1
        } else {
            year
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_get_water_year() {
        let date1 = Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap();
        assert_eq!(ReadingService::get_water_year(date1), 2025);

        let date2 = Utc.with_ymd_and_hms(2025, 9, 30, 23, 59, 59).unwrap();
        assert_eq!(ReadingService::get_water_year(date2), 2025);

        let date3 = Utc.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap();
        assert_eq!(ReadingService::get_water_year(date3), 2026);
    }
}

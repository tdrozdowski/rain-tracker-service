use chrono::{DateTime, Datelike, NaiveDate, Utc};
use std::collections::HashMap;

use crate::db::{
    CalendarYearSummary, DbError, MonthlySummary, Reading, ReadingRepository, WaterYearSummary,
};

#[derive(Clone)]
pub struct ReadingService {
    reading_repo: ReadingRepository,
}

impl ReadingService {
    pub fn new(reading_repo: ReadingRepository) -> Self {
        Self { reading_repo }
    }

    /// Get water year summary with business logic
    pub async fn get_water_year_summary(
        &self,
        water_year: i32,
    ) -> Result<WaterYearSummary, DbError> {
        // Calculate date range (business logic)
        let (start, end) = Self::water_year_date_range(water_year);

        // Fetch data (repository)
        let readings = self.reading_repo.find_by_date_range(start, end).await?;

        // Calculate summary (business logic)
        let total_rainfall = Self::calculate_total_rainfall(&readings);

        Ok(WaterYearSummary {
            water_year,
            total_readings: readings.len(),
            total_rainfall_inches: total_rainfall,
            readings,
        })
    }

    /// Get calendar year summary with monthly breakdowns
    pub async fn get_calendar_year_summary(
        &self,
        year: i32,
    ) -> Result<CalendarYearSummary, DbError> {
        // Calculate date range (business logic)
        let (start, end) = Self::calendar_year_date_range(year);

        // Fetch data (repository)
        let mut readings = self.reading_repo.find_by_date_range(start, end).await?;

        // Sort and calculate (business logic)
        readings.sort_by_key(|r| r.reading_datetime);
        let monthly_summaries = Self::calculate_monthly_summaries(&readings);
        let year_to_date_rainfall = monthly_summaries
            .iter()
            .rev()
            .find(|m| m.readings_count > 0)
            .map(|m| m.cumulative_ytd_inches)
            .unwrap_or(0.0);

        readings.reverse(); // Back to desc for API

        Ok(CalendarYearSummary {
            calendar_year: year,
            total_readings: readings.len(),
            year_to_date_rainfall_inches: year_to_date_rainfall,
            monthly_summaries,
            readings,
        })
    }

    /// Get latest reading
    pub async fn get_latest_reading(&self) -> Result<Option<Reading>, DbError> {
        self.reading_repo.find_latest().await
    }

    // Business logic helpers (private)

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

    fn calendar_year_date_range(year: i32) -> (DateTime<Utc>, DateTime<Utc>) {
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

    fn calculate_total_rainfall(readings: &[Reading]) -> f64 {
        readings.iter().map(|r| r.incremental_inches).sum()
    }

    fn calculate_monthly_summaries(readings: &[Reading]) -> Vec<MonthlySummary> {
        // Group readings by month
        let mut monthly_data: HashMap<u32, Vec<&Reading>> = HashMap::new();
        for reading in readings {
            let month = reading.reading_datetime.month();
            monthly_data
                .entry(month)
                .or_insert_with(Vec::new)
                .push(reading);
        }

        // Find the last reading in September (end of previous water year) to get baseline for Oct-Dec
        let sept_final_cumulative = if let Some(sept_readings) = monthly_data.get(&9) {
            // Get the latest reading in September
            sept_readings
                .iter()
                .max_by_key(|r| r.reading_datetime)
                .map(|r| r.cumulative_inches)
                .unwrap_or(0.0)
        } else {
            0.0
        };

        // Calculate monthly summaries with cumulative values
        let mut summaries = Vec::new();
        let mut cumulative_jan_sept = 0.0; // Accumulator for Jan-Sept (water year portion in calendar year)
        let mut cumulative_oct_dec = 0.0; // Accumulator for Oct-Dec (new water year)

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
                    month_name: Self::get_month_name(month),
                    readings_count: month_readings.len(),
                    monthly_rainfall_inches: month_rainfall,
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
                    month_name: Self::get_month_name(month),
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

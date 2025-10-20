use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::{debug, instrument};

use crate::db::{DbError, MonthlyRainfallSummary, Reading};

#[derive(Clone)]
pub struct MonthlyRainfallRepository {
    pool: PgPool,
}

impl MonthlyRainfallRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Upsert monthly rainfall summary for a specific month
    /// This is called when new readings are inserted
    #[instrument(skip(self))]
    pub async fn upsert_monthly_summary(
        &self,
        station_id: &str,
        year: i32,
        month: i32,
        readings: &[Reading],
    ) -> Result<(), DbError> {
        if readings.is_empty() {
            debug!("No readings to process for {}-{:02}", year, month);
            return Ok(());
        }

        // Calculate aggregates from readings
        let total_rainfall: f64 = readings.iter().map(|r| r.incremental_inches).sum();
        let reading_count = readings.len() as i32;

        let first_reading_date = readings
            .iter()
            .min_by_key(|r| r.reading_datetime)
            .map(|r| r.reading_datetime);

        let last_reading_date = readings
            .iter()
            .max_by_key(|r| r.reading_datetime)
            .map(|r| r.reading_datetime);

        let min_cumulative = readings
            .iter()
            .map(|r| r.cumulative_inches)
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        let max_cumulative = readings
            .iter()
            .map(|r| r.cumulative_inches)
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap_or(0.0);

        sqlx::query!(
            r#"
            INSERT INTO monthly_rainfall_summary
                (station_id, year, month, total_rainfall_inches, reading_count,
                 first_reading_date, last_reading_date, min_cumulative_inches, max_cumulative_inches)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (station_id, year, month)
            DO UPDATE SET
                total_rainfall_inches = EXCLUDED.total_rainfall_inches,
                reading_count = EXCLUDED.reading_count,
                first_reading_date = EXCLUDED.first_reading_date,
                last_reading_date = EXCLUDED.last_reading_date,
                min_cumulative_inches = EXCLUDED.min_cumulative_inches,
                max_cumulative_inches = EXCLUDED.max_cumulative_inches,
                updated_at = NOW()
            "#,
            station_id,
            year,
            month,
            total_rainfall,
            reading_count,
            first_reading_date,
            last_reading_date,
            min_cumulative,
            max_cumulative
        )
        .execute(&self.pool)
        .await?;

        debug!(
            "Upserted monthly summary for {} {}-{:02}: {} inches from {} readings",
            station_id, year, month, total_rainfall, reading_count
        );

        Ok(())
    }

    /// Get monthly summaries for a year (calendar year)
    #[instrument(skip(self))]
    pub async fn get_calendar_year_summaries(
        &self,
        station_id: &str,
        year: i32,
    ) -> Result<Vec<MonthlyRainfallSummary>, DbError> {
        let summaries = sqlx::query_as!(
            MonthlyRainfallSummary,
            r#"
            SELECT id, station_id, year, month, total_rainfall_inches, reading_count,
                   first_reading_date, last_reading_date, min_cumulative_inches, max_cumulative_inches,
                   created_at, updated_at
            FROM monthly_rainfall_summary
            WHERE station_id = $1 AND year = $2
            ORDER BY month ASC
            "#,
            station_id,
            year
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(summaries)
    }

    /// Get monthly summaries for a water year (Oct prev year - Sep current year)
    #[instrument(skip(self))]
    pub async fn get_water_year_summaries(
        &self,
        station_id: &str,
        water_year: i32,
    ) -> Result<Vec<MonthlyRainfallSummary>, DbError> {
        let summaries = sqlx::query_as!(
            MonthlyRainfallSummary,
            r#"
            SELECT id, station_id, year, month, total_rainfall_inches, reading_count,
                   first_reading_date, last_reading_date, min_cumulative_inches, max_cumulative_inches,
                   created_at, updated_at
            FROM monthly_rainfall_summary
            WHERE station_id = $1
              AND ((year = $2 - 1 AND month >= 10) OR (year = $2 AND month <= 9))
            ORDER BY year ASC, month ASC
            "#,
            station_id,
            water_year
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(summaries)
    }

    /// Recalculate monthly summary from raw readings
    /// Useful for backfilling or correcting data
    #[instrument(skip(self))]
    pub async fn recalculate_monthly_summary(
        &self,
        station_id: &str,
        year: i32,
        month: i32,
    ) -> Result<(), DbError> {
        // Fetch all readings for this month from rain_readings table
        let start_date = chrono::NaiveDate::from_ymd_opt(year, month as u32, 1)
            .unwrap()
            .and_hms_opt(0, 0, 0)
            .unwrap();
        let start_dt = DateTime::<Utc>::from_naive_utc_and_offset(start_date, Utc);

        let end_date = if month == 12 {
            chrono::NaiveDate::from_ymd_opt(year + 1, 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        } else {
            chrono::NaiveDate::from_ymd_opt(year, month as u32 + 1, 1)
                .unwrap()
                .and_hms_opt(0, 0, 0)
                .unwrap()
        };
        let end_dt = DateTime::<Utc>::from_naive_utc_and_offset(end_date, Utc);

        let readings = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE station_id = $1 AND reading_datetime >= $2 AND reading_datetime < $3
            ORDER BY reading_datetime ASC
            "#,
            station_id,
            start_dt,
            end_dt
        )
        .fetch_all(&self.pool)
        .await?;

        self.upsert_monthly_summary(station_id, year, month, &readings)
            .await
    }
}

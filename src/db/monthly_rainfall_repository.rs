use chrono::{DateTime, Utc};
use sqlx::{PgPool, Postgres, Transaction};
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

    /// Get monthly summaries by date range
    ///
    /// Generic data access method - business logic for water years, calendar years, etc.
    /// should be in the service layer.
    #[instrument(skip(self))]
    pub async fn get_summaries_by_date_range(
        &self,
        station_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MonthlyRainfallSummary>, DbError> {
        let summaries = sqlx::query_as!(
            MonthlyRainfallSummary,
            r#"
            SELECT id, station_id, year, month, total_rainfall_inches, reading_count,
                   first_reading_date, last_reading_date, min_cumulative_inches, max_cumulative_inches,
                   created_at, updated_at
            FROM monthly_rainfall_summary
            WHERE station_id = $1
              AND (
                (year > EXTRACT(YEAR FROM $2::timestamptz) OR
                 (year = EXTRACT(YEAR FROM $2::timestamptz) AND month >= EXTRACT(MONTH FROM $2::timestamptz)))
                AND
                (year < EXTRACT(YEAR FROM $3::timestamptz) OR
                 (year = EXTRACT(YEAR FROM $3::timestamptz) AND month < EXTRACT(MONTH FROM $3::timestamptz)))
              )
            ORDER BY year ASC, month ASC
            "#,
            station_id,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(summaries)
    }

    /// Recalculate monthly summary from raw readings by date range
    ///
    /// Pure data access method - service layer should calculate date boundaries.
    /// Useful for backfilling or correcting data.
    #[instrument(skip(self))]
    pub async fn recalculate_monthly_summary(
        &self,
        station_id: &str,
        year: i32,
        month: i32,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), DbError> {
        debug!(
            "Recalculating monthly summary for {} {}-{:02}",
            station_id, year, month
        );

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
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        self.upsert_monthly_summary(station_id, year, month, &readings)
            .await
    }

    // ============================================================
    // Transaction-aware methods for testing
    // ============================================================

    /// Upsert monthly summary using a transaction (for testing)
    #[instrument(skip(self, tx))]
    pub async fn upsert_monthly_summary_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        station_id: &str,
        year: i32,
        month: i32,
        readings: &[Reading],
    ) -> Result<(), DbError> {
        if readings.is_empty() {
            debug!("No readings to process for {}-{:02}", year, month);
            return Ok(());
        }

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
        .execute(&mut **tx)
        .await?;

        debug!(
            "Upserted monthly summary for {} {}-{:02}: {} inches from {} readings",
            station_id, year, month, total_rainfall, reading_count
        );

        Ok(())
    }

    /// Get monthly summaries by date range using a transaction (for testing)
    #[instrument(skip(self, tx))]
    pub async fn get_summaries_by_date_range_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        station_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<MonthlyRainfallSummary>, DbError> {
        let summaries = sqlx::query_as!(
            MonthlyRainfallSummary,
            r#"
            SELECT id, station_id, year, month, total_rainfall_inches, reading_count,
                   first_reading_date, last_reading_date, min_cumulative_inches, max_cumulative_inches,
                   created_at, updated_at
            FROM monthly_rainfall_summary
            WHERE station_id = $1
              AND (
                (year > EXTRACT(YEAR FROM $2::timestamptz) OR
                 (year = EXTRACT(YEAR FROM $2::timestamptz) AND month >= EXTRACT(MONTH FROM $2::timestamptz)))
                AND
                (year < EXTRACT(YEAR FROM $3::timestamptz) OR
                 (year = EXTRACT(YEAR FROM $3::timestamptz) AND month < EXTRACT(MONTH FROM $3::timestamptz)))
              )
            ORDER BY year ASC, month ASC
            "#,
            station_id,
            start,
            end
        )
        .fetch_all(&mut **tx)
        .await?;

        Ok(summaries)
    }

    /// Recalculate monthly summary using a transaction (for testing)
    #[instrument(skip(self, tx))]
    pub async fn recalculate_monthly_summary_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        station_id: &str,
        year: i32,
        month: i32,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), DbError> {
        debug!(
            "Recalculating monthly summary for {} {}-{:02}",
            station_id, year, month
        );

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
            start,
            end
        )
        .fetch_all(&mut **tx)
        .await?;

        self.upsert_monthly_summary_tx(tx, station_id, year, month, &readings)
            .await
    }
}

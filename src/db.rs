use chrono::{DateTime, Datelike, NaiveDate, Utc};
use serde::Serialize;
use sqlx::{FromRow, PgPool};
use tracing::{debug, info, instrument};

use crate::fetcher::RainReading;

#[derive(Debug, Clone, FromRow, Serialize)]
pub struct StoredReading {
    pub id: i64,
    pub reading_datetime: DateTime<Utc>,
    pub cumulative_inches: f64,
    pub incremental_inches: f64,
    pub station_id: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
    #[error("Database error: {0}")]
    SqlxError(#[from] sqlx::Error),
}

#[derive(Clone)]
pub struct RainDb {
    pool: PgPool,
}

impl RainDb {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[instrument(skip(self, readings), fields(count = readings.len()))]
    pub async fn insert_readings(&self, readings: &[RainReading]) -> Result<usize, DbError> {
        debug!("Beginning transaction to insert {} readings", readings.len());
        let mut tx = self.pool.begin().await?;
        let mut inserted = 0;
        let mut duplicates = 0;

        for reading in readings {
            let result = sqlx::query!(
                r#"
                INSERT INTO rain_readings (reading_datetime, cumulative_inches, incremental_inches)
                VALUES ($1, $2, $3)
                ON CONFLICT (reading_datetime, station_id) DO NOTHING
                "#,
                reading.reading_datetime,
                reading.cumulative_inches,
                reading.incremental_inches
            )
            .execute(&mut *tx)
            .await?;

            if result.rows_affected() > 0 {
                inserted += 1;
            } else {
                duplicates += 1;
            }
        }

        tx.commit().await?;
        info!(
            "Inserted {} new readings, {} duplicates skipped",
            inserted, duplicates
        );
        Ok(inserted)
    }

    #[instrument(skip(self), fields(water_year = %water_year))]
    pub async fn get_water_year_readings(&self, water_year: i32) -> Result<Vec<StoredReading>, DbError> {
        // Water year starts Oct 1 of (water_year - 1) and ends Sep 30 of water_year
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

        debug!("Querying water year {} (from {} to {})", water_year, start_dt, end_dt);

        let readings = sqlx::query_as!(
            StoredReading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE reading_datetime >= $1 AND reading_datetime < $2
            ORDER BY reading_datetime DESC
            "#,
            start_dt,
            end_dt
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} readings for water year {}", readings.len(), water_year);
        Ok(readings)
    }

    #[instrument(skip(self), fields(year = %year))]
    pub async fn get_calendar_year_readings(&self, year: i32) -> Result<Vec<StoredReading>, DbError> {
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

        debug!("Querying calendar year {} (from {} to {})", year, start_dt, end_dt);

        let readings = sqlx::query_as!(
            StoredReading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE reading_datetime >= $1 AND reading_datetime < $2
            ORDER BY reading_datetime DESC
            "#,
            start_dt,
            end_dt
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} readings for calendar year {}", readings.len(), year);
        Ok(readings)
    }

    #[instrument(skip(self))]
    pub async fn get_latest_reading(&self) -> Result<Option<StoredReading>, DbError> {
        debug!("Querying for latest reading");
        let reading = sqlx::query_as!(
            StoredReading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            ORDER BY reading_datetime DESC
            LIMIT 1
            "#
        )
        .fetch_optional(&self.pool)
        .await?;

        if reading.is_some() {
            debug!("Found latest reading");
        } else {
            debug!("No readings found in database");
        }

        Ok(reading)
    }
}

/// Calculates which rain year a given date falls into
pub fn get_water_year(date: DateTime<Utc>) -> i32 {
    let year = date.year();
    let month = date.month();

    if month >= 10 {
        year + 1
    } else {
        year
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn test_get_water_year() {
        let date1 = Utc.with_ymd_and_hms(2024, 10, 1, 0, 0, 0).unwrap();
        assert_eq!(get_water_year(date1), 2025);

        let date2 = Utc.with_ymd_and_hms(2025, 9, 30, 23, 59, 59).unwrap();
        assert_eq!(get_water_year(date2), 2025);

        let date3 = Utc.with_ymd_and_hms(2025, 10, 1, 0, 0, 0).unwrap();
        assert_eq!(get_water_year(date3), 2026);
    }
}

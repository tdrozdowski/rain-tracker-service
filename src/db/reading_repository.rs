use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::db::{DbError, Reading};
use crate::fetcher::RainReading;

#[derive(Clone)]
pub struct ReadingRepository {
    pool: PgPool,
}

impl ReadingRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert multiple readings in a transaction
    #[instrument(skip(self, readings), fields(count = readings.len()))]
    pub async fn insert_readings(&self, readings: &[RainReading]) -> Result<usize, DbError> {
        debug!(
            "Beginning transaction to insert {} readings",
            readings.len()
        );
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

    /// Generic query to find readings within a date range
    /// Business logic for water years, calendar years, etc. should be in service layer
    #[instrument(skip(self))]
    pub async fn find_by_date_range(
        &self,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Reading>, DbError> {
        debug!("Querying readings from {} to {}", start, end);

        let readings = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE reading_datetime >= $1 AND reading_datetime < $2
            ORDER BY reading_datetime DESC
            "#,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} readings", readings.len());
        Ok(readings)
    }

    /// Find the most recent reading
    #[instrument(skip(self))]
    pub async fn find_latest(&self) -> Result<Option<Reading>, DbError> {
        debug!("Querying for latest reading");

        let reading = sqlx::query_as!(
            Reading,
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

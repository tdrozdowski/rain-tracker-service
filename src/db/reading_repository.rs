use chrono::{DateTime, Datelike, TimeZone, Utc};
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::db::{DbError, Reading};
use crate::fetcher::RainReading;
use crate::importers::excel_importer::HistoricalReading;

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

    /// Insert historical readings (from FOPR imports, Excel files, etc.) in bulk
    ///
    /// This is a data access method - all business logic should be in the service layer.
    /// Returns (inserted_count, duplicate_count, affected_months) where affected_months
    /// contains (year, month) tuples for months that had new data inserted.
    #[instrument(skip(self, readings), fields(station_id = %station_id, count = readings.len()))]
    #[allow(clippy::type_complexity)]
    pub async fn bulk_insert_historical_readings(
        &self,
        station_id: &str,
        data_source: &str,
        readings: &[HistoricalReading],
    ) -> Result<(usize, usize, Vec<(i32, u32)>), DbError> {
        debug!(
            "Bulk inserting {} historical readings for station {} from source {}",
            readings.len(),
            station_id,
            data_source
        );

        let mut inserted = 0;
        let mut duplicates = 0;
        let mut affected_months = Vec::new();

        for reading in readings {
            let import_metadata = reading.footnote_marker.as_ref().map(|marker| {
                serde_json::json!({
                    "footnote_marker": marker
                })
            });

            // Convert NaiveDate to DateTime<Utc> for midnight
            let reading_datetime =
                Utc.from_utc_datetime(&reading.reading_date.and_hms_opt(0, 0, 0).unwrap());

            let result = sqlx::query!(
                r#"
                INSERT INTO rain_readings (station_id, reading_datetime, cumulative_inches, incremental_inches, data_source, import_metadata)
                VALUES ($1, $2, $3, $4, $5, $6)
                ON CONFLICT (reading_datetime, station_id) DO NOTHING
                "#,
                station_id,
                reading_datetime,
                0.0, // FOPR files only have incremental, cumulative is calculated separately
                reading.rainfall_inches,
                data_source,
                import_metadata as _
            )
            .execute(&self.pool)
            .await?;

            if result.rows_affected() > 0 {
                inserted += 1;
                let year = reading.reading_date.year();
                let month = reading.reading_date.month();
                affected_months.push((year, month));
            } else {
                duplicates += 1;
            }
        }

        info!(
            "Bulk insert complete: {} inserted, {} duplicates for station {}",
            inserted, duplicates, station_id
        );

        Ok((inserted, duplicates, affected_months))
    }

    /// Generic query to find readings within a date range for a specific gauge
    /// Business logic for water years, calendar years, etc. should be in service layer
    #[instrument(skip(self))]
    pub async fn find_by_date_range(
        &self,
        station_id: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<Vec<Reading>, DbError> {
        debug!(
            "Querying readings for gauge {} from {} to {}",
            station_id, start, end
        );

        let readings = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE station_id = $1 AND reading_datetime >= $2 AND reading_datetime < $3
            ORDER BY reading_datetime DESC
            "#,
            station_id,
            start,
            end
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} readings for gauge {}", readings.len(), station_id);
        Ok(readings)
    }

    /// Find the most recent reading for a specific gauge
    #[instrument(skip(self))]
    pub async fn find_latest(&self, station_id: &str) -> Result<Option<Reading>, DbError> {
        debug!("Querying for latest reading for gauge {}", station_id);

        let reading = sqlx::query_as!(
            Reading,
            r#"
            SELECT id, reading_datetime, cumulative_inches as "cumulative_inches!",
                   incremental_inches as "incremental_inches!", station_id, created_at
            FROM rain_readings
            WHERE station_id = $1
            ORDER BY reading_datetime DESC
            LIMIT 1
            "#,
            station_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if reading.is_some() {
            debug!("Found latest reading for gauge {}", station_id);
        } else {
            debug!("No readings found for gauge {}", station_id);
        }

        Ok(reading)
    }
}

use chrono::{Datelike, TimeZone, Utc};
use sqlx::PgPool;
use std::collections::HashSet;
use std::io::Write;
use std::time::Instant;
use tracing::{debug, error, info, instrument, warn};

use crate::db::fopr_import_job_repository::{FoprImportJobRepository, ImportStats};
use crate::db::{DbError, GaugeRepository, MonthlyRainfallRepository};
use crate::fopr::daily_data_parser::FoprDailyDataParser;
use crate::fopr::metadata_parser::MetaStatsData;
use crate::importers::downloader::McfcdDownloader;
use crate::importers::excel_importer::HistoricalReading;

/// Error types for FOPR import operations
#[derive(Debug, thiserror::Error)]
pub enum FoprImportError {
    #[error("Download failed: {0}")]
    Download(String),

    #[error("Parse failed: {0}")]
    Parse(String),

    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("Gauge not found: {0}")]
    GaugeNotFound(String),

    #[error("No readings found in FOPR file")]
    NoReadings,
}

/// Service for importing FOPR (Full Operational Period of Record) data
pub struct FoprImportService {
    pool: PgPool,
    downloader: McfcdDownloader,
    gauge_repo: GaugeRepository,
    monthly_repo: MonthlyRainfallRepository,
    job_repo: FoprImportJobRepository,
}

impl FoprImportService {
    pub fn new(pool: PgPool) -> Self {
        Self {
            gauge_repo: GaugeRepository::new(pool.clone()),
            monthly_repo: MonthlyRainfallRepository::new(pool.clone()),
            job_repo: FoprImportJobRepository::new(pool.clone()),
            downloader: McfcdDownloader::new(),
            pool,
        }
    }

    /// Import FOPR data for a gauge
    ///
    /// This is the main business logic method that:
    /// 1. Downloads FOPR file
    /// 2. Parses metadata and upserts gauge
    /// 3. Parses all year sheets
    /// 4. Inserts readings with deduplication
    /// 5. Recalculates monthly summaries
    /// 6. Returns import statistics
    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn import_fopr(&self, station_id: &str) -> Result<ImportStats, FoprImportError> {
        let start_time = Instant::now();
        info!("Starting FOPR import for station {}", station_id);

        // 1. Download FOPR file
        info!("Downloading FOPR file for station {}", station_id);
        let fopr_bytes = self
            .downloader
            .download_fopr(station_id)
            .await
            .map_err(|e| FoprImportError::Download(e.to_string()))?;

        info!(
            "Downloaded FOPR file ({} bytes) for station {}",
            fopr_bytes.len(),
            station_id
        );

        // 2. Write to temp file (calamine requires file path)
        let mut temp_file = tempfile::NamedTempFile::new()?;
        temp_file.write_all(&fopr_bytes)?;
        let temp_path = temp_file.path().to_string_lossy().to_string();

        // 3. Parse and upsert gauge metadata
        info!("Parsing gauge metadata from Meta_Stats sheet");
        let metadata = {
            use calamine::{open_workbook, Reader, Xlsx};
            use std::fs::File;
            use std::io::BufReader;

            let mut workbook: Xlsx<BufReader<File>> = open_workbook(&temp_path)
                .map_err(|e| FoprImportError::Parse(format!("Failed to open workbook: {e}")))?;

            let range = workbook.worksheet_range("Meta_Stats").map_err(|e| {
                FoprImportError::Parse(format!("Failed to read Meta_Stats sheet: {e:?}"))
            })?;

            MetaStatsData::from_worksheet_range(&range)
                .map_err(|e| FoprImportError::Parse(format!("Metadata parse error: {e}")))?
        };

        info!(
            "Parsed metadata for station {} ({})",
            metadata.station_id, metadata.station_name
        );

        self.gauge_repo
            .upsert_gauge_metadata(&metadata)
            .await
            .map_err(|e| {
                let DbError::SqlxError(sqlx_err) = e;
                FoprImportError::Database(sqlx_err)
            })?;

        info!("Upserted gauge metadata for station {}", station_id);

        // 4. Parse all year sheets
        info!("Parsing daily rainfall data from year sheets");
        let data_parser = FoprDailyDataParser::new(&temp_path, station_id);
        let readings = data_parser
            .parse_all_years()
            .map_err(|e| FoprImportError::Parse(format!("Daily data parse error: {e}")))?;

        if readings.is_empty() {
            warn!("No readings found in FOPR file for station {}", station_id);
            return Err(FoprImportError::NoReadings);
        }

        info!(
            "Parsed {} readings for station {}",
            readings.len(),
            station_id
        );

        // 5. Insert readings with deduplication
        let (inserted, duplicates, months_to_recalc) =
            self.insert_readings_bulk(station_id, readings).await?;

        info!(
            "Inserted {} readings, {} duplicates for station {}",
            inserted, duplicates, station_id
        );

        // 6. Recalculate monthly summaries
        if !months_to_recalc.is_empty() {
            info!(
                "Recalculating {} monthly summaries for station {}",
                months_to_recalc.len(),
                station_id
            );
            self.recalculate_monthly_summaries(&months_to_recalc)
                .await?;
        }

        let duration = start_time.elapsed();
        info!(
            "✓ FOPR import complete for station {} ({:.1}s, {} readings)",
            station_id,
            duration.as_secs_f64(),
            inserted
        );

        // Build statistics
        let stats = ImportStats {
            readings_imported: inserted as i64,
            start_date: None, // Could calculate from readings if needed
            end_date: None,
            duration_secs: duration.as_secs_f64(),
        };

        Ok(stats)
    }

    /// Insert readings in bulk with deduplication
    ///
    /// Returns: (inserted_count, duplicate_count, months_to_recalculate)
    #[instrument(skip(self, readings), fields(station_id = %station_id, count = readings.len()))]
    #[allow(clippy::type_complexity)]
    async fn insert_readings_bulk(
        &self,
        station_id: &str,
        readings: Vec<HistoricalReading>,
    ) -> Result<(usize, usize, HashSet<(String, i32, u32)>), FoprImportError> {
        debug!(
            "Inserting {} readings for station {}",
            readings.len(),
            station_id
        );

        let mut inserted = 0;
        let mut duplicates = 0;
        let mut months_to_recalculate: HashSet<(String, i32, u32)> = HashSet::new();

        for reading in readings {
            let data_source = format!("fopr_import_{station_id}");
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
                reading.station_id,
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
                months_to_recalculate.insert((station_id.to_string(), year, month));
            } else {
                duplicates += 1;
            }
        }

        debug!(
            "Insert complete: {} inserted, {} duplicates",
            inserted, duplicates
        );

        Ok((inserted, duplicates, months_to_recalculate))
    }

    /// Recalculate monthly summaries for affected station-months
    #[instrument(skip(self, months), fields(count = months.len()))]
    async fn recalculate_monthly_summaries(
        &self,
        months: &HashSet<(String, i32, u32)>,
    ) -> Result<(), FoprImportError> {
        debug!("Recalculating {} monthly summaries", months.len());

        for (station_id, year, month) in months {
            self.monthly_repo
                .recalculate_monthly_summary(station_id, *year, *month as i32)
                .await
                .map_err(|e| {
                    let DbError::SqlxError(sqlx_err) = e;
                    FoprImportError::Database(sqlx_err)
                })?;
        }

        debug!("Monthly summaries recalculated");
        Ok(())
    }

    /// Check if FOPR import job already exists for a station
    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn job_exists(&self, station_id: &str) -> Result<bool, FoprImportError> {
        self.job_repo.job_exists(station_id).await.map_err(|e| {
            let DbError::SqlxError(sqlx_err) = e;
            FoprImportError::Database(sqlx_err)
        })
    }
}

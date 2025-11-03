use sqlx::{PgPool, Postgres, Transaction};
use tracing::{debug, error, info, instrument};

use crate::db::{DbError, GaugeSummary};
use crate::fopr::MetaStatsData;
use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;

#[derive(Clone)]
pub struct GaugeRepository {
    pool: PgPool,
}

impl GaugeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    #[instrument(skip(self, summaries), fields(count = summaries.len()))]
    pub async fn upsert_summaries(&self, summaries: &[FetchedGauge]) -> Result<usize, DbError> {
        debug!(
            "Beginning transaction to upsert {} gauge summaries",
            summaries.len()
        );
        let mut tx = self.pool.begin().await?;
        let mut upserted = 0;

        for summary in summaries {
            let result = sqlx::query!(
                r#"
                INSERT INTO gauge_summaries (
                    station_id, gauge_name, city_town, elevation_ft,
                    general_location, msp_forecast_zone,
                    rainfall_past_6h_inches, rainfall_past_24h_inches,
                    last_scraped_at, updated_at
                )
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, NOW(), NOW())
                ON CONFLICT (station_id) DO UPDATE SET
                    gauge_name = EXCLUDED.gauge_name,
                    city_town = EXCLUDED.city_town,
                    elevation_ft = EXCLUDED.elevation_ft,
                    general_location = EXCLUDED.general_location,
                    msp_forecast_zone = EXCLUDED.msp_forecast_zone,
                    rainfall_past_6h_inches = EXCLUDED.rainfall_past_6h_inches,
                    rainfall_past_24h_inches = EXCLUDED.rainfall_past_24h_inches,
                    last_scraped_at = NOW(),
                    updated_at = NOW()
                "#,
                summary.station_id,
                summary.gauge_name,
                summary.city_town,
                summary.elevation_ft,
                summary.general_location,
                summary.msp_forecast_zone,
                summary.rainfall_past_6h_inches,
                summary.rainfall_past_24h_inches
            )
            .execute(&mut *tx)
            .await
            .map_err(|e| {
                error!(
                    station_id = %summary.station_id,
                    gauge_name = %summary.gauge_name,
                    error = %e,
                    "Failed to upsert gauge summary"
                );
                e
            })?;

            upserted += result.rows_affected() as usize;
        }

        tx.commit().await?;
        debug!("Successfully upserted {} gauge summaries", upserted);
        Ok(upserted)
    }

    #[instrument(skip(self))]
    pub async fn count(&self) -> Result<usize, DbError> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM gauge_summaries")
            .fetch_one(&self.pool)
            .await?;

        Ok(count.unwrap_or(0) as usize)
    }

    #[instrument(skip(self))]
    pub async fn find_paginated(
        &self,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<GaugeSummary>, DbError> {
        debug!("Querying gauges with offset={}, limit={}", offset, limit);

        let gauges = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            ORDER BY city_town, gauge_name
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} gauges", gauges.len());
        Ok(gauges)
    }

    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn find_by_id(&self, station_id: &str) -> Result<Option<GaugeSummary>, DbError> {
        debug!("Querying gauge by station_id");

        let gauge = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            WHERE station_id = $1
            "#,
            station_id
        )
        .fetch_optional(&self.pool)
        .await?;

        if gauge.is_some() {
            debug!("Found gauge");
        } else {
            debug!("Gauge not found");
        }

        Ok(gauge)
    }

    /// Upsert gauge metadata from FOPR Meta_Stats sheet
    ///
    /// This inserts a new gauge or updates existing gauge metadata.
    /// Used during FOPR imports to ensure gauge exists before importing readings.
    #[instrument(skip(self, metadata), fields(station_id = %metadata.station_id))]
    pub async fn upsert_gauge_metadata(&self, metadata: &MetaStatsData) -> Result<(), DbError> {
        info!(
            "Upserting gauge metadata for station {}",
            metadata.station_id
        );

        // Use untyped query to avoid bigdecimal dependency for DECIMAL columns
        let result = sqlx::query(
            r#"
            INSERT INTO gauges (
                station_id, station_name, station_type, previous_station_ids,
                latitude, longitude, elevation_ft, county, city, location_description,
                installation_date, data_begins_date, status,
                avg_annual_precipitation_inches, complete_years_count,
                incomplete_months_count, missing_months_count, data_quality_remarks,
                fopr_metadata, metadata_source, metadata_updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, 'fopr_import', NOW())
            ON CONFLICT (station_id) DO UPDATE SET
                station_name = EXCLUDED.station_name,
                station_type = EXCLUDED.station_type,
                previous_station_ids = EXCLUDED.previous_station_ids,
                latitude = EXCLUDED.latitude,
                longitude = EXCLUDED.longitude,
                elevation_ft = EXCLUDED.elevation_ft,
                county = EXCLUDED.county,
                city = EXCLUDED.city,
                location_description = EXCLUDED.location_description,
                installation_date = EXCLUDED.installation_date,
                data_begins_date = EXCLUDED.data_begins_date,
                status = EXCLUDED.status,
                avg_annual_precipitation_inches = EXCLUDED.avg_annual_precipitation_inches,
                complete_years_count = EXCLUDED.complete_years_count,
                incomplete_months_count = EXCLUDED.incomplete_months_count,
                missing_months_count = EXCLUDED.missing_months_count,
                data_quality_remarks = EXCLUDED.data_quality_remarks,
                fopr_metadata = EXCLUDED.fopr_metadata,
                metadata_source = 'fopr_import',
                metadata_updated_at = NOW()
            "#
        )
        .bind(&metadata.station_id)
        .bind(&metadata.station_name)
        .bind(&metadata.station_type)
        .bind(&metadata.previous_station_ids)
        .bind(metadata.latitude)
        .bind(metadata.longitude)
        .bind(metadata.elevation_ft)
        .bind(&metadata.county)
        .bind(&metadata.city)
        .bind(&metadata.location_description)
        .bind(metadata.installation_date)
        .bind(metadata.data_begins_date)
        .bind(&metadata.status)
        .bind(metadata.avg_annual_precipitation_inches)
        .bind(metadata.complete_years_count)
        .bind(metadata.incomplete_months_count)
        .bind(metadata.missing_months_count)
        .bind(&metadata.data_quality_remarks)
        .bind(serde_json::to_value(&metadata.fopr_metadata).unwrap())
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            info!(
                "Successfully upserted gauge metadata for station {}",
                metadata.station_id
            );
        }

        Ok(())
    }

    /// Check if a gauge exists by station_id
    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn gauge_exists(&self, station_id: &str) -> Result<bool, DbError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM gauges WHERE station_id = $1"#,
            station_id
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.unwrap_or(0) > 0)
    }

    // ============================================================
    // Transaction-aware methods for testing
    // ============================================================

    /// Upsert gauge metadata using a transaction (for testing)
    #[instrument(skip(self, tx, metadata), fields(station_id = %metadata.station_id))]
    pub async fn upsert_gauge_metadata_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        metadata: &MetaStatsData,
    ) -> Result<(), DbError> {
        info!(
            "Upserting gauge metadata for station {}",
            metadata.station_id
        );

        let result = sqlx::query(
            r#"
            INSERT INTO gauges (
                station_id, station_name, station_type, previous_station_ids,
                latitude, longitude, elevation_ft, county, city, location_description,
                installation_date, data_begins_date, status,
                avg_annual_precipitation_inches, complete_years_count,
                incomplete_months_count, missing_months_count, data_quality_remarks,
                fopr_metadata, metadata_source, metadata_updated_at
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, 'fopr_import', NOW())
            ON CONFLICT (station_id) DO UPDATE SET
                station_name = EXCLUDED.station_name,
                station_type = EXCLUDED.station_type,
                previous_station_ids = EXCLUDED.previous_station_ids,
                latitude = EXCLUDED.latitude,
                longitude = EXCLUDED.longitude,
                elevation_ft = EXCLUDED.elevation_ft,
                county = EXCLUDED.county,
                city = EXCLUDED.city,
                location_description = EXCLUDED.location_description,
                installation_date = EXCLUDED.installation_date,
                data_begins_date = EXCLUDED.data_begins_date,
                status = EXCLUDED.status,
                avg_annual_precipitation_inches = EXCLUDED.avg_annual_precipitation_inches,
                complete_years_count = EXCLUDED.complete_years_count,
                incomplete_months_count = EXCLUDED.incomplete_months_count,
                missing_months_count = EXCLUDED.missing_months_count,
                data_quality_remarks = EXCLUDED.data_quality_remarks,
                fopr_metadata = EXCLUDED.fopr_metadata,
                metadata_source = 'fopr_import',
                metadata_updated_at = NOW()
            "#
        )
        .bind(&metadata.station_id)
        .bind(&metadata.station_name)
        .bind(&metadata.station_type)
        .bind(&metadata.previous_station_ids)
        .bind(metadata.latitude)
        .bind(metadata.longitude)
        .bind(metadata.elevation_ft)
        .bind(&metadata.county)
        .bind(&metadata.city)
        .bind(&metadata.location_description)
        .bind(metadata.installation_date)
        .bind(metadata.data_begins_date)
        .bind(&metadata.status)
        .bind(metadata.avg_annual_precipitation_inches)
        .bind(metadata.complete_years_count)
        .bind(metadata.incomplete_months_count)
        .bind(metadata.missing_months_count)
        .bind(&metadata.data_quality_remarks)
        .bind(serde_json::to_value(&metadata.fopr_metadata).unwrap())
        .execute(&mut **tx)
        .await?;

        if result.rows_affected() > 0 {
            info!(
                "Successfully upserted gauge metadata for station {}",
                metadata.station_id
            );
        }

        Ok(())
    }

    /// Find gauge by ID using a transaction (for testing)
    #[instrument(skip(self, tx), fields(station_id = %station_id))]
    pub async fn find_by_id_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        station_id: &str,
    ) -> Result<Option<GaugeSummary>, DbError> {
        debug!("Querying gauge by station_id");

        let gauge = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            WHERE station_id = $1
            "#,
            station_id
        )
        .fetch_optional(&mut **tx)
        .await?;

        if gauge.is_some() {
            debug!("Found gauge");
        } else {
            debug!("Gauge not found");
        }

        Ok(gauge)
    }

    /// Check if a gauge exists using a transaction (for testing)
    #[instrument(skip(self, tx), fields(station_id = %station_id))]
    pub async fn gauge_exists_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        station_id: &str,
    ) -> Result<bool, DbError> {
        let count = sqlx::query_scalar!(
            r#"SELECT COUNT(*) FROM gauges WHERE station_id = $1"#,
            station_id
        )
        .fetch_one(&mut **tx)
        .await?;

        Ok(count.unwrap_or(0) > 0)
    }

    /// Count gauges using a transaction (for testing)
    #[instrument(skip(self, tx))]
    pub async fn count_tx(&self, tx: &mut Transaction<'_, Postgres>) -> Result<usize, DbError> {
        let count = sqlx::query_scalar!("SELECT COUNT(*) FROM gauge_summaries")
            .fetch_one(&mut **tx)
            .await?;

        Ok(count.unwrap_or(0) as usize)
    }

    /// Find paginated gauges using a transaction (for testing)
    #[instrument(skip(self, tx))]
    pub async fn find_paginated_tx(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        offset: i64,
        limit: i64,
    ) -> Result<Vec<GaugeSummary>, DbError> {
        debug!("Querying gauges with offset={}, limit={}", offset, limit);

        let gauges = sqlx::query_as!(
            GaugeSummary,
            r#"
            SELECT id, station_id, gauge_name, city_town, elevation_ft,
                   general_location, msp_forecast_zone,
                   rainfall_past_6h_inches, rainfall_past_24h_inches,
                   last_scraped_at, created_at, updated_at
            FROM gauge_summaries
            ORDER BY city_town, gauge_name
            LIMIT $1 OFFSET $2
            "#,
            limit,
            offset
        )
        .fetch_all(&mut **tx)
        .await?;

        debug!("Found {} gauges", gauges.len());
        Ok(gauges)
    }
}

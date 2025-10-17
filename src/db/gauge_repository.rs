use sqlx::PgPool;
use tracing::{debug, instrument};

use crate::db::{DbError, GaugeSummary};

#[derive(Clone)]
pub struct GaugeRepository {
    pool: PgPool,
}

impl GaugeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert or update gauge summaries (upsert based on station_id)
    /// Note: This method signature expects a FetchedGauge type that will be created
    /// in the gauge_list_fetcher module. For now, this is a placeholder.
    #[instrument(skip(self))]
    pub async fn count(&self) -> Result<usize, DbError> {
        let count = sqlx::query_scalar!(
            "SELECT COUNT(*) FROM gauge_summaries"
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(count.unwrap_or(0) as usize)
    }

    #[instrument(skip(self))]
    pub async fn find_paginated(
        &self,
        offset: i64,
        limit: i64
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
}

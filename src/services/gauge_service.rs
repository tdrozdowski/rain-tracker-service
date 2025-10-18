use crate::db::{DbError, GaugeRepository, GaugeSummary};
use chrono::{DateTime, Utc};
use serde::Serialize;

// Pagination types (used by API)
#[derive(Debug, Clone, serde::Deserialize)]
pub struct PaginationParams {
    #[serde(default = "default_page")]
    pub page: u32,
    #[serde(default = "default_page_size")]
    pub page_size: u32,
}

fn default_page() -> u32 {
    1
}

fn default_page_size() -> u32 {
    50
}

impl PaginationParams {
    pub fn offset(&self) -> i64 {
        ((self.page - 1) * self.page_size) as i64
    }

    pub fn limit(&self) -> i64 {
        self.page_size.min(100) as i64
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct GaugeListResponse {
    pub total_gauges: usize,
    pub page: u32,
    pub page_size: u32,
    pub total_pages: u32,
    pub has_next_page: bool,
    pub has_prev_page: bool,
    pub last_scraped_at: Option<DateTime<Utc>>,
    pub gauges: Vec<GaugeSummary>,
}

#[derive(Clone)]
pub struct GaugeService {
    gauge_repo: GaugeRepository,
}

impl GaugeService {
    pub fn new(gauge_repo: GaugeRepository) -> Self {
        Self { gauge_repo }
    }

    /// Get paginated gauges with metadata
    pub async fn get_gauges_paginated(
        &self,
        params: &PaginationParams,
    ) -> Result<GaugeListResponse, DbError> {
        // Get data from repository
        let total_gauges = self.gauge_repo.count().await?;
        let gauges = self
            .gauge_repo
            .find_paginated(params.offset(), params.limit())
            .await?;

        // Calculate pagination metadata (business logic)
        let total_pages = ((total_gauges as f64) / (params.page_size as f64)).ceil() as u32;
        let has_next_page = params.page < total_pages;
        let has_prev_page = params.page > 1;

        let last_scraped_at = gauges.iter().map(|g| g.last_scraped_at).max();

        Ok(GaugeListResponse {
            total_gauges,
            page: params.page,
            page_size: params.page_size,
            total_pages,
            has_next_page,
            has_prev_page,
            last_scraped_at,
            gauges,
        })
    }

    /// Get single gauge by ID
    pub async fn get_gauge_by_id(&self, station_id: &str) -> Result<Option<GaugeSummary>, DbError> {
        self.gauge_repo.find_by_id(station_id).await
    }
}

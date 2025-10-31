use crate::db::fopr_import_job_repository::FoprImportJobRepository;
use crate::db::{DbError, GaugeRepository, GaugeSummary};
use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;
use chrono::{DateTime, Utc};
use serde::Serialize;
use tracing::{debug, info, instrument};
use utoipa::{IntoParams, ToSchema};

// Pagination types (used by API)
#[derive(Debug, Clone, serde::Deserialize, IntoParams)]
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

#[derive(Debug, Clone, Serialize, ToSchema)]
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
    job_repo: FoprImportJobRepository,
}

impl GaugeService {
    pub fn new(gauge_repo: GaugeRepository, job_repo: FoprImportJobRepository) -> Self {
        Self {
            gauge_repo,
            job_repo,
        }
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

    /// Handle discovery of a new gauge from scraper
    ///
    /// This method is called when the gauge list scraper discovers a gauge.
    /// If the gauge doesn't exist in the gauges table:
    /// 1. Check if an import job already exists
    /// 2. If not, create a new FOPR import job
    /// 3. Store the gauge summary in the job
    ///
    /// Returns true if a new job was created.
    #[instrument(skip(self, gauge_summary), fields(station_id = %gauge_summary.station_id))]
    pub async fn handle_new_gauge_discovery(
        &self,
        gauge_summary: &FetchedGauge,
    ) -> Result<bool, DbError> {
        let station_id = &gauge_summary.station_id;
        debug!("Handling gauge discovery for station {}", station_id);

        // Check if gauge exists in gauges table (has metadata)
        let gauge_exists = self.gauge_repo.gauge_exists(station_id).await?;

        if gauge_exists {
            debug!("Gauge {} already exists, no action needed", station_id);
            return Ok(false);
        }

        info!(
            "New gauge discovered: {} - checking for existing job",
            station_id
        );

        // Check if import job already exists
        let job_exists = self.job_repo.job_exists(station_id).await?;

        if job_exists {
            debug!("Import job already exists for station {}", station_id);
            return Ok(false);
        }

        // Create FOPR import job with gauge summary
        info!("Creating FOPR import job for new gauge {}", station_id);
        let job_id = self
            .job_repo
            .create_job(
                station_id,
                "gauge_discovery",
                10, // Default priority
                Some(gauge_summary),
            )
            .await?;

        info!(
            "Created FOPR import job {} for new gauge {}",
            job_id, station_id
        );
        Ok(true)
    }

    /// Upsert gauge summaries (called by scheduler after scraping)
    ///
    /// This updates the gauge_summaries table with the latest scraped data.
    /// Note: This is separate from the gauges table which contains full metadata.
    #[instrument(skip(self, summaries), fields(count = summaries.len()))]
    pub async fn upsert_summaries(&self, summaries: &[FetchedGauge]) -> Result<usize, DbError> {
        debug!("Upserting {} gauge summaries", summaries.len());
        self.gauge_repo.upsert_summaries(summaries).await
    }
}

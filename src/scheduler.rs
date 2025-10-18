use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, instrument, warn};

use crate::db::{GaugeRepository, ReadingRepository};
use crate::fetcher::RainGaugeFetcher;
use crate::gauge_list_fetcher::GaugeListFetcher;

#[instrument(skip(fetcher, reading_repo), fields(interval_minutes = %interval_minutes))]
pub async fn start_fetch_scheduler(
    fetcher: RainGaugeFetcher,
    reading_repo: ReadingRepository,
    interval_minutes: u64,
) {
    let mut interval = time::interval(Duration::from_secs(interval_minutes * 60));

    info!(
        "Fetch scheduler started with {} minute interval",
        interval_minutes
    );

    loop {
        interval.tick().await;
        debug!("Scheduler tick - initiating fetch");

        match fetch_and_store(&fetcher, &reading_repo).await {
            Ok(inserted) => {
                if inserted > 0 {
                    info!("Successfully fetched and stored {} new readings", inserted);
                } else {
                    debug!("No new readings to store (all duplicates)");
                }
            }
            Err(e) => {
                error!("Failed to fetch and store readings: {}", e);
            }
        }
    }
}

#[instrument(skip(fetcher, reading_repo))]
async fn fetch_and_store(
    fetcher: &RainGaugeFetcher,
    reading_repo: &ReadingRepository,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching readings from gauge");
    let readings = fetcher.fetch_readings().await?;
    info!("Fetched {} readings from gauge", readings.len());

    if readings.is_empty() {
        warn!("No readings returned from gauge");
    }

    debug!("Inserting readings into database");
    let inserted = reading_repo.insert_readings(&readings).await?;
    Ok(inserted)
}

#[instrument(skip(fetcher, gauge_repo), fields(interval_minutes = %interval_minutes))]
pub async fn start_gauge_list_scheduler(
    fetcher: GaugeListFetcher,
    gauge_repo: GaugeRepository,
    interval_minutes: u64,
) {
    let mut interval = time::interval(Duration::from_secs(interval_minutes * 60));

    info!(
        "Gauge list scheduler started with {} minute interval",
        interval_minutes
    );

    loop {
        interval.tick().await;
        debug!("Gauge list scheduler tick - initiating fetch");

        match fetch_and_store_gauge_list(&fetcher, &gauge_repo).await {
            Ok(count) => {
                info!("Successfully fetched and stored {} gauge summaries", count);
            }
            Err(e) => {
                error!("Failed to fetch gauge list: {}", e);
            }
        }
    }
}

#[instrument(skip(fetcher, gauge_repo))]
async fn fetch_and_store_gauge_list(
    fetcher: &GaugeListFetcher,
    gauge_repo: &GaugeRepository,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching gauge list");
    let gauges = fetcher.fetch_gauge_list().await?;
    info!("Fetched {} gauges from list", gauges.len());

    debug!("Upserting gauge summaries into database");
    let upserted = gauge_repo.upsert_summaries(&gauges).await?;
    Ok(upserted)
}

use chrono::Datelike;
use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, instrument, warn};

use crate::db::{MonthlyRainfallRepository, ReadingRepository};
use crate::fetcher::RainGaugeFetcher;
use crate::gauge_list_fetcher::GaugeListFetcher;
use crate::services::gauge_service::GaugeService;

#[instrument(skip(fetcher, reading_repo, monthly_repo), fields(interval_minutes = %interval_minutes))]
pub async fn start_fetch_scheduler(
    fetcher: RainGaugeFetcher,
    reading_repo: ReadingRepository,
    monthly_repo: MonthlyRainfallRepository,
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

        match fetch_and_store(&fetcher, &reading_repo, &monthly_repo).await {
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

#[instrument(skip(fetcher, reading_repo, monthly_repo))]
async fn fetch_and_store(
    fetcher: &RainGaugeFetcher,
    reading_repo: &ReadingRepository,
    monthly_repo: &MonthlyRainfallRepository,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching readings from gauge");
    let readings = fetcher.fetch_readings().await?;
    info!("Fetched {} readings from gauge", readings.len());

    if readings.is_empty() {
        warn!("No readings returned from gauge");
        return Ok(0);
    }

    debug!("Inserting readings into database");
    let inserted = reading_repo.insert_readings(&readings).await?;

    if inserted > 0 {
        // Update monthly aggregates for affected months
        // Group readings by month and recalculate
        use std::collections::HashMap;
        let mut months_to_update: HashMap<(i32, i32), ()> = HashMap::new();

        for reading in &readings {
            let year = reading.reading_datetime.year();
            let month = reading.reading_datetime.month() as i32;
            months_to_update.insert((year, month), ());
        }

        debug!(
            "Updating {} affected monthly summaries",
            months_to_update.len()
        );
        for ((year, month), _) in months_to_update {
            // Use the default station_id (59700) since RainGaugeFetcher doesn't expose station_id
            // TODO: Make station_id configurable in fetcher
            if let Err(e) = monthly_repo
                .recalculate_monthly_summary("59700", year, month)
                .await
            {
                error!(
                    "Failed to update monthly summary for {}-{:02}: {}",
                    year, month, e
                );
            }
        }
    }

    Ok(inserted)
}

#[instrument(skip(fetcher, gauge_service), fields(interval_minutes = %interval_minutes))]
pub async fn start_gauge_list_scheduler(
    fetcher: GaugeListFetcher,
    gauge_service: GaugeService,
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

        match fetch_and_store_gauge_list(&fetcher, &gauge_service).await {
            Ok(count) => {
                info!("Successfully fetched and stored {} gauge summaries", count);
            }
            Err(e) => {
                error!("Failed to fetch gauge list: {}", e);
            }
        }
    }
}

#[instrument(skip(fetcher, gauge_service))]
async fn fetch_and_store_gauge_list(
    fetcher: &GaugeListFetcher,
    gauge_service: &GaugeService,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching gauge list");
    let gauges = fetcher.fetch_gauge_list().await?;
    info!("Fetched {} gauges from list", gauges.len());

    // Handle new gauge discovery
    let mut new_jobs_created = 0;
    for gauge in &gauges {
        match gauge_service.handle_new_gauge_discovery(gauge).await {
            Ok(true) => {
                info!("Created FOPR import job for new gauge {}", gauge.station_id);
                new_jobs_created += 1;
            }
            Ok(false) => {
                // Gauge already exists or job already created
            }
            Err(e) => {
                error!(
                    "Failed to handle discovery for gauge {}: {}",
                    gauge.station_id, e
                );
            }
        }
    }

    if new_jobs_created > 0 {
        info!(
            "Created {} FOPR import jobs for new gauges",
            new_jobs_created
        );
    }

    // Upsert gauge summaries
    debug!("Upserting gauge summaries into database");
    let upserted = gauge_service.upsert_summaries(&gauges).await?;
    Ok(upserted)
}

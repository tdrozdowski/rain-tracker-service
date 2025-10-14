use std::time::Duration;
use tokio::time;
use tracing::{debug, error, info, instrument, warn};

use crate::db::RainDb;
use crate::fetcher::RainGaugeFetcher;

#[instrument(skip(fetcher, db), fields(interval_minutes = %interval_minutes))]
pub async fn start_fetch_scheduler(
    fetcher: RainGaugeFetcher,
    db: RainDb,
    interval_minutes: u64,
) {
    let mut interval = time::interval(Duration::from_secs(interval_minutes * 60));

    info!("Fetch scheduler started with {} minute interval", interval_minutes);

    loop {
        interval.tick().await;
        debug!("Scheduler tick - initiating fetch");

        match fetch_and_store(&fetcher, &db).await {
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

#[instrument(skip(fetcher, db))]
async fn fetch_and_store(
    fetcher: &RainGaugeFetcher,
    db: &RainDb,
) -> Result<usize, Box<dyn std::error::Error>> {
    debug!("Fetching readings from gauge");
    let readings = fetcher.fetch_readings().await?;
    info!("Fetched {} readings from gauge", readings.len());

    if readings.is_empty() {
        warn!("No readings returned from gauge");
    }

    debug!("Inserting readings into database");
    let inserted = db.insert_readings(&readings).await?;
    Ok(inserted)
}

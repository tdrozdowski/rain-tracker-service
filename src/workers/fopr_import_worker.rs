use backon::{BackoffBuilder, ExponentialBuilder};
use chrono::Utc;
use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, instrument, warn};

use crate::db::fopr_import_job_repository::{ErrorHistoryEntry, FoprImportJobRepository};
use crate::services::fopr_import_service::FoprImportService;

/// FOPR Import Worker
///
/// This worker runs in the background, polling for pending FOPR import jobs
/// and executing them. It's a thin coordination layer - all business logic
/// is in FoprImportService.
pub struct FoprImportWorker {
    job_repo: FoprImportJobRepository,
    import_service: FoprImportService,
    poll_interval_secs: u64,
    worker_id: usize,
}

impl FoprImportWorker {
    pub fn new(
        job_repo: FoprImportJobRepository,
        import_service: FoprImportService,
        poll_interval_secs: u64,
        worker_id: usize,
    ) -> Self {
        Self {
            job_repo,
            import_service,
            poll_interval_secs,
            worker_id,
        }
    }

    /// Start the worker loop
    ///
    /// This runs indefinitely, polling for jobs at the configured interval.
    /// Each iteration:
    /// 1. Attempts to claim a job atomically
    /// 2. If claimed, executes the import
    /// 3. Updates job status based on result
    #[instrument(skip(self), fields(worker_id = %self.worker_id, poll_interval = %self.poll_interval_secs))]
    pub async fn run(&self) {
        info!(
            worker_id = self.worker_id,
            poll_interval_secs = self.poll_interval_secs,
            "FOPR import worker started"
        );

        let mut ticker = interval(Duration::from_secs(self.poll_interval_secs));

        loop {
            ticker.tick().await;

            if let Err(e) = self.process_next_job().await {
                error!(
                    worker_id = self.worker_id,
                    error = %e,
                    "Error processing job"
                );
            }
        }
    }

    /// Process a single job (if available)
    #[instrument(skip(self), fields(worker_id = %self.worker_id))]
    async fn process_next_job(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Atomically claim next job
        let job = match self.job_repo.claim_next_job().await? {
            Some(j) => j,
            None => {
                // No jobs available (this is normal, not worth logging at info level)
                return Ok(());
            }
        };

        info!(
            worker_id = self.worker_id,
            job_id = job.id,
            station_id = %job.station_id,
            "Claimed FOPR import job"
        );

        // Execute import
        let result = self.import_service.import_fopr(&job.station_id).await;

        // Update job based on result
        match result {
            Ok(stats) => {
                info!(
                    worker_id = self.worker_id,
                    job_id = job.id,
                    station_id = %job.station_id,
                    readings_imported = stats.readings_imported,
                    duration_secs = %format!("{:.2}", stats.duration_secs),
                    "Job completed successfully"
                );
                self.job_repo.mark_completed(job.id, &stats).await?;
            }
            Err(e) => {
                let error_msg = e.to_string();
                warn!(
                    worker_id = self.worker_id,
                    job_id = job.id,
                    station_id = %job.station_id,
                    error = %error_msg,
                    "Job failed"
                );

                let new_retry_count = job.retry_count + 1;

                // Business logic: Calculate retry schedule with exponential backoff using backon
                // Starts at 5 min, multiplies by 3x each time, caps at 45 min
                // Includes jitter to prevent thundering herd
                let backoff = ExponentialBuilder::default()
                    .with_min_delay(Duration::from_secs(5 * 60)) // Start: 5 minutes
                    .with_max_delay(Duration::from_secs(45 * 60)) // Cap: 45 minutes
                    .with_factor(3.0) // 5min -> 15min -> 45min
                    .with_jitter(); // Add randomness to prevent simultaneous retries

                // Calculate delay for this retry attempt
                // backon uses 0-indexed attempts, so retry_count 1 = attempt 0
                let delay = backoff
                    .build()
                    .nth(new_retry_count.saturating_sub(1) as usize)
                    .unwrap_or(Duration::from_secs(45 * 60)); // Fallback to max if calculation fails

                let next_retry_at = Utc::now()
                    + chrono::Duration::from_std(delay).unwrap_or(chrono::Duration::minutes(45));

                // Business logic: Construct error history entry
                let error_entry = ErrorHistoryEntry {
                    timestamp: Utc::now(),
                    error: error_msg.clone(),
                    retry_count: new_retry_count,
                };

                self.job_repo
                    .mark_failed(
                        job.id,
                        &error_msg,
                        &error_entry,
                        new_retry_count,
                        next_retry_at,
                    )
                    .await?;

                if new_retry_count >= job.max_retries {
                    error!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        station_id = %job.station_id,
                        retry_count = new_retry_count,
                        max_retries = job.max_retries,
                        "Job exceeded max retries, giving up"
                    );
                } else {
                    info!(
                        worker_id = self.worker_id,
                        job_id = job.id,
                        station_id = %job.station_id,
                        retry_count = new_retry_count,
                        max_retries = job.max_retries,
                        next_retry_at = %next_retry_at,
                        "Job will be retried"
                    );
                }
            }
        }

        Ok(())
    }
}

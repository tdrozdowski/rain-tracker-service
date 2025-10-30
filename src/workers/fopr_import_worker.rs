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
}

impl FoprImportWorker {
    pub fn new(
        job_repo: FoprImportJobRepository,
        import_service: FoprImportService,
        poll_interval_secs: u64,
    ) -> Self {
        Self {
            job_repo,
            import_service,
            poll_interval_secs,
        }
    }

    /// Start the worker loop
    ///
    /// This runs indefinitely, polling for jobs at the configured interval.
    /// Each iteration:
    /// 1. Attempts to claim a job atomically
    /// 2. If claimed, executes the import
    /// 3. Updates job status based on result
    #[instrument(skip(self), fields(poll_interval = %self.poll_interval_secs))]
    pub async fn run(&self) {
        info!(
            "Starting FOPR import worker (poll interval: {}s)",
            self.poll_interval_secs
        );

        let mut ticker = interval(Duration::from_secs(self.poll_interval_secs));

        loop {
            ticker.tick().await;

            if let Err(e) = self.process_next_job().await {
                error!("Error processing job: {}", e);
            }
        }
    }

    /// Process a single job (if available)
    #[instrument(skip(self))]
    async fn process_next_job(&self) -> Result<(), Box<dyn std::error::Error>> {
        // Atomically claim next job
        let job = match self.job_repo.claim_next_job().await? {
            Some(j) => j,
            None => {
                // No jobs available
                return Ok(());
            }
        };

        info!(
            "Processing FOPR import job {} for station {}",
            job.id, job.station_id
        );

        // Execute import
        let result = self.import_service.import_fopr(&job.station_id).await;

        // Update job based on result
        match result {
            Ok(stats) => {
                info!(
                    "✓ Job {} completed successfully ({} readings imported)",
                    job.id, stats.readings_imported
                );
                self.job_repo.mark_completed(job.id, &stats).await?;
            }
            Err(e) => {
                let error_msg = e.to_string();
                warn!("Job {} failed: {}", job.id, error_msg);

                let new_retry_count = job.retry_count + 1;

                // Business logic: Calculate retry schedule with exponential backoff
                // 5 min, 15 min, 45 min
                let retry_delay_secs = match new_retry_count {
                    1 => 5 * 60,  // 5 minutes
                    2 => 15 * 60, // 15 minutes
                    _ => 45 * 60, // 45 minutes
                };
                let next_retry_at = Utc::now() + chrono::Duration::seconds(retry_delay_secs);

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
                        "Job {} exceeded max retries ({}), giving up",
                        job.id, job.max_retries
                    );
                } else {
                    info!(
                        "Job {} will be retried (attempt {}/{}) at {}",
                        job.id, new_retry_count, job.max_retries, next_retry_at
                    );
                }
            }
        }

        Ok(())
    }
}

use std::time::Duration;
use tokio::time::interval;
use tracing::{error, info, instrument, warn};

use crate::db::fopr_import_job_repository::FoprImportJobRepository;
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
                self.job_repo
                    .mark_failed(job.id, &error_msg, new_retry_count)
                    .await?;

                if new_retry_count >= job.max_retries {
                    error!(
                        "Job {} exceeded max retries ({}), giving up",
                        job.id, job.max_retries
                    );
                } else {
                    info!(
                        "Job {} will be retried (attempt {}/{})",
                        job.id, new_retry_count, job.max_retries
                    );
                }
            }
        }

        Ok(())
    }
}

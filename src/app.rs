use sqlx::PgPool;
use tokio::task::JoinHandle;
use tower_http::trace::TraceLayer;
use tracing::info;

use crate::api::{create_router, AppState};
use crate::config::Config;
use crate::db::fopr_import_job_repository::FoprImportJobRepository;
use crate::db::{GaugeRepository, MonthlyRainfallRepository, ReadingRepository};
use crate::fetcher::RainGaugeFetcher;
use crate::gauge_list_fetcher::GaugeListFetcher;
use crate::scheduler;
use crate::services::fopr_import_service::FoprImportService;
use crate::services::{GaugeService, ReadingService};
use crate::workers::fopr_import_worker::FoprImportWorker;

/// Application with all spawned background tasks and server
///
/// This struct holds handles to all running tasks, allowing graceful
/// shutdown if needed. For now, tasks run indefinitely.
pub struct Application {
    pub server_handle: JoinHandle<Result<(), std::io::Error>>,
    pub reading_scheduler_handle: JoinHandle<()>,
    pub gauge_list_scheduler_handle: JoinHandle<()>,
    pub fopr_worker_handles: Vec<JoinHandle<()>>,
}

impl Application {
    /// Build and initialize the application
    ///
    /// This creates all services, repositories, fetchers, and spawns:
    /// - HTTP API server (Axum)
    /// - Reading scheduler (15 min interval)
    /// - Gauge list scheduler (60 min interval)
    /// - FOPR import workers (configurable concurrency, default 10)
    pub async fn build(config: Config, pool: PgPool) -> Result<Self, Box<dyn std::error::Error>> {
        info!("Initializing application components");

        // Create repositories
        let reading_repo = ReadingRepository::new(pool.clone());
        let gauge_repo = GaugeRepository::new(pool.clone());
        let monthly_rainfall_repo = MonthlyRainfallRepository::new(pool.clone());
        let job_repo = FoprImportJobRepository::new(pool.clone());

        // Create services
        let reading_service =
            ReadingService::new(reading_repo.clone(), monthly_rainfall_repo.clone());
        let gauge_service = GaugeService::new(gauge_repo.clone(), job_repo.clone());
        let fopr_import_service = FoprImportService::new(pool.clone());

        // Create fetchers
        let reading_fetcher = RainGaugeFetcher::new(config.gauge_url.clone());
        let gauge_list_fetcher = GaugeListFetcher::new(config.gauge_list_url.clone());

        // Spawn background tasks
        info!("Spawning background schedulers and workers");
        info!(
            "FOPR worker concurrency: {} workers",
            config.fopr_worker_concurrency
        );

        // Scheduler 1: Individual gauge readings (15 min interval)
        let reading_scheduler_handle = {
            let reading_repo_clone = reading_repo.clone();
            let monthly_repo_clone = monthly_rainfall_repo.clone();
            let reading_fetcher_clone = reading_fetcher.clone();
            let reading_interval = config.fetch_interval_minutes;

            tokio::spawn(async move {
                scheduler::start_fetch_scheduler(
                    reading_fetcher_clone,
                    reading_repo_clone,
                    monthly_repo_clone,
                    reading_interval,
                )
                .await;
            })
        };

        // Scheduler 2: Gauge list/summaries (60 min interval)
        let gauge_list_scheduler_handle = {
            let gauge_service_clone = gauge_service.clone();
            let gauge_list_fetcher_clone = gauge_list_fetcher.clone();
            let gauge_list_interval = config.gauge_list_interval_minutes;

            tokio::spawn(async move {
                scheduler::start_gauge_list_scheduler(
                    gauge_list_fetcher_clone,
                    gauge_service_clone,
                    gauge_list_interval,
                )
                .await;
            })
        };

        // Workers: FOPR import workers (spawn multiple for concurrent processing)
        let mut fopr_worker_handles = Vec::new();
        for worker_id in 0..config.fopr_worker_concurrency {
            let worker = FoprImportWorker::new(
                job_repo.clone(),
                fopr_import_service.clone(),
                30, // Poll every 30 seconds
                worker_id,
            );

            let handle = tokio::spawn(async move {
                worker.run().await;
            });

            fopr_worker_handles.push(handle);
        }

        // Create API router
        let app_state = AppState {
            reading_service,
            gauge_service,
        };
        let app = create_router(app_state).layer(TraceLayer::new_for_http());

        // Spawn server
        let addr = config.server_addr();
        info!("Starting HTTP server on {}", addr);

        let server_handle = tokio::spawn(async move {
            let listener = tokio::net::TcpListener::bind(&addr).await?;
            axum::serve(listener, app).await
        });

        info!("Application initialized successfully");

        Ok(Self {
            server_handle,
            reading_scheduler_handle,
            gauge_list_scheduler_handle,
            fopr_worker_handles,
        })
    }

    /// Run until the server stops (which runs indefinitely unless error)
    ///
    /// Background schedulers and workers also run indefinitely.
    pub async fn run_until_stopped(self) -> Result<(), Box<dyn std::error::Error>> {
        // Wait for server (the main task)
        // Schedulers and worker run indefinitely in background
        self.server_handle.await??;
        Ok(())
    }
}

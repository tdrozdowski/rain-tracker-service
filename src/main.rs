use sqlx::postgres::PgPoolOptions;
use tower_http::trace::TraceLayer;
use tracing::{info, instrument};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

use rain_tracker_service::api::{create_router, AppState};
use rain_tracker_service::config::Config;
use rain_tracker_service::db::{ReadingRepository, GaugeRepository};
use rain_tracker_service::services::ReadingService;
use rain_tracker_service::fetcher::RainGaugeFetcher;
use rain_tracker_service::scheduler;

#[tokio::main]
#[instrument]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize tracing with environment filter support
    tracing_subscriber::registry()
        .with(
            EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| EnvFilter::new("info,rain_tracker_service=debug"))
        )
        .with(
            tracing_subscriber::fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_line_number(true)
        )
        .init();

    // Load environment variables
    dotenvy::dotenv().ok();

    // Load configuration
    let config = Config::from_env()?;
    info!("Starting rain tracker service with config: {:?}", config);

    // Create database connection pool
    info!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;
    info!("Database connection established");

    // Run migrations
    info!("Running database migrations...");
    sqlx::migrate!("./migrations").run(&pool).await?;
    info!("Database migrations completed");

    // Create repositories
    let reading_repo = ReadingRepository::new(pool.clone());
    let _gauge_repo = GaugeRepository::new(pool.clone());

    // Create services
    let reading_service = ReadingService::new(reading_repo.clone());
    // Note: GaugeService will be used in Phase 2 when we add gauge endpoints
    // let _gauge_service = GaugeService::new(gauge_repo);

    // Create fetcher
    let fetcher = RainGaugeFetcher::new(config.gauge_url.clone());

    // Start background scheduler
    info!("Starting background fetch scheduler");
    let reading_repo_clone = reading_repo.clone();
    let fetcher_clone = fetcher.clone();
    let interval = config.fetch_interval_minutes;
    tokio::spawn(async move {
        scheduler::start_fetch_scheduler(fetcher_clone, reading_repo_clone, interval).await;
    });

    // Create API router
    let app_state = AppState { reading_service };
    let app = create_router(app_state).layer(TraceLayer::new_for_http());

    // Start server
    let addr = config.server_addr();
    info!("Server listening on {}", addr);

    let listener = tokio::net::TcpListener::bind(&addr).await?;
    axum::serve(listener, app).await?;

    Ok(())
}

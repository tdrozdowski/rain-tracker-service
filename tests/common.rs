use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::OnceCell;

static POOL: OnceCell<PgPool> = OnceCell::const_new();

/// Get a shared connection pool for all tests
/// Pool is created once and reused across tests
pub async fn test_pool() -> &'static PgPool {
    POOL.get_or_init(|| async {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(20) // Increased for parallel tests
            .acquire_timeout(std::time::Duration::from_secs(60))
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations once
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Clean up any leftover test data
        sqlx::query("TRUNCATE TABLE monthly_rainfall_summary, rain_readings CASCADE")
            .execute(&pool)
            .await
            .ok();

        pool
    })
    .await
}

/// Begin a test transaction that will automatically rollback
pub async fn test_transaction() -> Transaction<'static, Postgres> {
    test_pool()
        .await
        .begin()
        .await
        .expect("Failed to begin transaction")
}

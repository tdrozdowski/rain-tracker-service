use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres, Transaction};
use tokio::sync::OnceCell;

static INIT: OnceCell<()> = OnceCell::const_new();

/// Initialize the test database (run migrations, cleanup)
/// This runs once per test run
async fn init_test_db() {
    INIT.get_or_init(|| async {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
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

        pool.close().await;
    })
    .await;
}

/// Get a NEW connection pool for each test
/// This avoids connection exhaustion issues
pub async fn test_pool() -> PgPool {
    init_test_db().await;

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
    });

    PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .expect("Failed to connect to test database")
}

/// Begin a test transaction that will automatically rollback
pub async fn test_transaction() -> Transaction<'static, Postgres> {
    // Create a pool and leak it so the transaction can have 'static lifetime
    let pool = Box::leak(Box::new(test_pool().await));
    pool.begin().await.expect("Failed to begin transaction")
}

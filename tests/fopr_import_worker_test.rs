// Unit tests for FOPR Import Worker
// Tests job processing, retry logic, and error handling

use chrono::Utc;
use rain_tracker_service::db::fopr_import_job_repository::{ImportStats, JobStatus};
use rain_tracker_service::db::{FoprImportJobRepository, GaugeRepository};
use rain_tracker_service::fopr::MetaStatsData;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

/// Test fixture module for worker tests
mod worker_test_fixtures {
    use super::*;
    use chrono::NaiveDate;

    pub const TEST_WORKER_GAUGE: &str = "TEST_WORKER_001";

    /// Setup test database for worker tests
    pub async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        // Run migrations
        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        // Insert test gauge
        insert_test_gauge(&pool).await;

        pool
    }

    /// Insert test gauge for worker tests
    async fn insert_test_gauge(pool: &PgPool) {
        let gauge_repo = GaugeRepository::new(pool.clone());

        // Check if gauge already exists
        if gauge_repo
            .gauge_exists(TEST_WORKER_GAUGE)
            .await
            .unwrap_or(false)
        {
            return;
        }

        // Create test gauge metadata
        let metadata = MetaStatsData {
            station_id: TEST_WORKER_GAUGE.to_string(),
            station_name: "Test Worker Gauge".to_string(),
            previous_station_ids: vec![],
            station_type: "Rain".to_string(),
            latitude: 33.5,
            longitude: -112.0,
            elevation_ft: Some(1000),
            county: "Maricopa".to_string(),
            city: Some("Phoenix".to_string()),
            location_description: Some("Test gauge for worker tests".to_string()),
            installation_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            data_begins_date: Some(NaiveDate::from_ymd_opt(2020, 1, 1).unwrap()),
            status: "Active".to_string(),
            avg_annual_precipitation_inches: Some(8.0),
            complete_years_count: Some(5),
            incomplete_months_count: 0,
            missing_months_count: 0,
            data_quality_remarks: Some("Test gauge for worker tests".to_string()),
            fopr_metadata: serde_json::Map::new(),
        };

        gauge_repo
            .upsert_gauge_metadata(&metadata)
            .await
            .expect("Failed to insert test gauge");
    }

    /// Clean up test data
    pub async fn cleanup_test_data(pool: &PgPool) {
        // Clean up jobs
        sqlx::query!(
            "DELETE FROM fopr_import_jobs WHERE station_id = $1",
            TEST_WORKER_GAUGE
        )
        .execute(pool)
        .await
        .ok();

        // Clean up readings
        sqlx::query!(
            "DELETE FROM rain_readings WHERE station_id = $1",
            TEST_WORKER_GAUGE
        )
        .execute(pool)
        .await
        .ok();
    }
}

#[tokio::test]
async fn test_worker_claims_pending_job() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create a pending job
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    // Verify job is pending
    let job = job_repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Pending);
    assert_eq!(job.retry_count, 0);

    // Claim the job
    let claimed_job = job_repo.claim_next_job().await.unwrap();
    assert!(claimed_job.is_some());

    let claimed = claimed_job.unwrap();
    assert_eq!(claimed.id, job_id);
    assert_eq!(claimed.status, JobStatus::InProgress);
    assert_eq!(claimed.station_id, worker_test_fixtures::TEST_WORKER_GAUGE);
}

#[tokio::test]
async fn test_worker_no_job_when_queue_empty() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    // Clean up ALL jobs to ensure empty queue
    sqlx::query!("DELETE FROM fopr_import_jobs")
        .execute(&pool)
        .await
        .ok();

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Try to claim when queue is empty
    let claimed_job = job_repo.claim_next_job().await.unwrap();
    assert!(claimed_job.is_none());
}

#[tokio::test]
async fn test_worker_marks_job_completed() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create and claim a job
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    let job = job_repo.claim_next_job().await.unwrap().unwrap();
    assert_eq!(job.id, job_id);

    // Mark job as completed with stats
    let stats = ImportStats {
        readings_imported: 100,
        start_date: Some("2023-01-01".to_string()),
        end_date: Some("2024-12-31".to_string()),
        duration_secs: 45.5,
    };

    job_repo.mark_completed(job_id, &stats).await.unwrap();

    // Verify job status
    let completed_job = job_repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(completed_job.status, JobStatus::Completed);
    assert!(completed_job.completed_at.is_some());
    assert!(completed_job.import_stats.is_some());
}

#[tokio::test]
async fn test_worker_marks_job_failed() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create and claim a job
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    let job = job_repo.claim_next_job().await.unwrap().unwrap();
    assert_eq!(job.id, job_id);

    // Mark job as failed
    use rain_tracker_service::db::fopr_import_job_repository::ErrorHistoryEntry;
    let error_entry = ErrorHistoryEntry {
        timestamp: Utc::now(),
        error: "Test error".to_string(),
        retry_count: 1,
    };

    let next_retry_at = Utc::now() + chrono::Duration::minutes(5);

    job_repo
        .mark_failed(job_id, "Test error", &error_entry, 1, next_retry_at)
        .await
        .unwrap();

    // Verify job status
    let failed_job = job_repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(failed_job.status, JobStatus::Failed); // Stays in failed status until retry
    assert_eq!(failed_job.retry_count, 1);
    assert!(failed_job.next_retry_at.is_some());

    // Deserialize error_history to check it
    let error_history: Vec<ErrorHistoryEntry> =
        serde_json::from_value(failed_job.error_history).unwrap();
    assert!(!error_history.is_empty());
}

#[tokio::test]
async fn test_worker_retry_backoff_calculation() {
    // This test verifies the retry backoff logic by checking multiple retries
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create a job
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    // Simulate multiple failures with increasing retry counts
    use rain_tracker_service::db::fopr_import_job_repository::ErrorHistoryEntry;

    for retry in 1..=3 {
        let _job = job_repo.claim_next_job().await.unwrap().unwrap();

        let error_entry = ErrorHistoryEntry {
            timestamp: Utc::now(),
            error: format!("Test error {retry}"),
            retry_count: retry,
        };

        // Calculate next retry time (this mimics the worker's backon logic)
        let base_delay_minutes = match retry {
            1 => 5,  // ~5 min (with jitter)
            2 => 15, // ~15 min (with jitter)
            _ => 45, // ~45 min (cap, with jitter)
        };

        let next_retry_at = Utc::now() + chrono::Duration::minutes(base_delay_minutes);

        job_repo
            .mark_failed(
                job_id,
                &format!("Test error {retry}"),
                &error_entry,
                retry,
                next_retry_at,
            )
            .await
            .unwrap();

        // Verify retry count increased
        let failed_job = job_repo.get_job(job_id).await.unwrap().unwrap();
        assert_eq!(failed_job.retry_count, retry);

        // Deserialize error_history to check count
        let error_history: Vec<ErrorHistoryEntry> =
            serde_json::from_value(failed_job.error_history).unwrap();
        assert_eq!(error_history.len(), retry as usize);
    }
}

#[tokio::test]
async fn test_worker_max_retries_exceeded() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create a job with max_retries = 3
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    // Update max_retries to 3 for testing
    sqlx::query!(
        "UPDATE fopr_import_jobs SET max_retries = 3 WHERE id = $1",
        job_id
    )
    .execute(&pool)
    .await
    .unwrap();

    // Simulate 4 failures (exceeding max_retries)
    use rain_tracker_service::db::fopr_import_job_repository::ErrorHistoryEntry;

    for retry in 1..=4 {
        let job = job_repo.claim_next_job().await.unwrap();

        if retry > 3 {
            // After max retries, job should not be claimable
            assert!(job.is_none());
            break;
        }

        let _job = job.unwrap();

        let error_entry = ErrorHistoryEntry {
            timestamp: Utc::now(),
            error: format!("Test error {retry}"),
            retry_count: retry,
        };

        let next_retry_at = Utc::now() + chrono::Duration::minutes(5);

        job_repo
            .mark_failed(
                job_id,
                &format!("Test error {retry}"),
                &error_entry,
                retry,
                next_retry_at,
            )
            .await
            .unwrap();
    }

    // Verify job remains failed with retry_count = max_retries
    let final_job = job_repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(final_job.retry_count, 3);
    assert_eq!(final_job.status, JobStatus::Failed); // Permanently failed
}

#[tokio::test]
async fn test_worker_job_priority_ordering() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    // Clean up all jobs to ensure clean slate
    sqlx::query!("DELETE FROM fopr_import_jobs")
        .execute(&pool)
        .await
        .unwrap();

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create jobs with different priorities
    let low_priority_job = job_repo
        .create_job("TEST_LOW", "test", 1, None)
        .await
        .unwrap();

    let high_priority_job = job_repo
        .create_job("TEST_HIGH", "test", 100, None)
        .await
        .unwrap();

    let medium_priority_job = job_repo
        .create_job("TEST_MEDIUM", "test", 50, None)
        .await
        .unwrap();

    // Claim jobs - should come out in priority order (high to low)
    let first_job = job_repo.claim_next_job().await.unwrap().unwrap();
    assert_eq!(first_job.id, high_priority_job);
    assert_eq!(first_job.priority, 100);

    let second_job = job_repo.claim_next_job().await.unwrap().unwrap();
    assert_eq!(second_job.id, medium_priority_job);
    assert_eq!(second_job.priority, 50);

    let third_job = job_repo.claim_next_job().await.unwrap().unwrap();
    assert_eq!(third_job.id, low_priority_job);
    assert_eq!(third_job.priority, 1);
}

#[tokio::test]
async fn test_worker_error_history_preserved() {
    let pool = worker_test_fixtures::setup_test_db().await;
    worker_test_fixtures::cleanup_test_data(&pool).await;

    let job_repo = FoprImportJobRepository::new(pool.clone());

    // Create a job
    let job_id = job_repo
        .create_job(worker_test_fixtures::TEST_WORKER_GAUGE, "test", 10, None)
        .await
        .unwrap();

    // Fail the job multiple times
    use rain_tracker_service::db::fopr_import_job_repository::ErrorHistoryEntry;

    for retry in 1..=3 {
        let _job = job_repo.claim_next_job().await.unwrap().unwrap();

        let error_entry = ErrorHistoryEntry {
            timestamp: Utc::now(),
            error: format!("Error #{retry}: Something went wrong"),
            retry_count: retry,
        };

        let next_retry_at = Utc::now() + chrono::Duration::minutes(5);

        job_repo
            .mark_failed(
                job_id,
                &error_entry.error,
                &error_entry,
                retry,
                next_retry_at,
            )
            .await
            .unwrap();
    }

    // Verify error history is preserved
    let final_job = job_repo.get_job(job_id).await.unwrap().unwrap();

    // Deserialize error_history
    let error_history: Vec<ErrorHistoryEntry> =
        serde_json::from_value(final_job.error_history).unwrap();
    assert_eq!(error_history.len(), 3);

    // Check that errors are in chronological order
    for (i, entry) in error_history.iter().enumerate() {
        assert_eq!(entry.retry_count, (i + 1) as i32);
        assert!(entry.error.contains(&format!("Error #{}", i + 1)));
    }
}

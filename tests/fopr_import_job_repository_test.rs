// Tests for FoprImportJobRepository to improve coverage
// Focus on transaction methods and uncovered paths

use chrono::Utc;
use rain_tracker_service::db::fopr_import_job_repository::{
    ErrorHistoryEntry, FoprImportJobRepository, ImportStats, JobStatus,
};
use serial_test::serial;
use sqlx::postgres::PgPoolOptions;
use sqlx::PgPool;

mod fopr_job_repo_fixtures {
    use super::*;

    pub async fn setup_test_db() -> PgPool {
        let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
            "postgres://postgres:password@localhost:5432/rain_tracker_test".to_string()
        });

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&database_url)
            .await
            .expect("Failed to connect to test database");

        sqlx::migrate!("./migrations")
            .run(&pool)
            .await
            .expect("Failed to run migrations");

        pool
    }

    pub async fn cleanup_jobs(pool: &PgPool, station_id: &str) {
        sqlx::query!(
            "DELETE FROM fopr_import_jobs WHERE station_id = $1",
            station_id
        )
        .execute(pool)
        .await
        .ok();
    }
}

#[tokio::test]
#[serial]
async fn test_get_pending_jobs_empty() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());

    let jobs = repo.get_pending_jobs().await.unwrap();

    // Should not error - the call succeeding is enough validation
    let _ = jobs;
}

#[tokio::test]
#[serial]
async fn test_get_pending_jobs_with_data() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "PENDING_TEST_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create a pending job
    let job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();

    let jobs = repo.get_pending_jobs().await.unwrap();

    // Should include our new job
    assert!(jobs.iter().any(|j| j.id == job_id));

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_create_job_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_CREATE_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Use transaction version
    let mut tx = pool.begin().await.unwrap();
    let job_id = repo
        .create_job_tx(&mut tx, station_id, "test", 1, None)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Verify job was created
    let job = repo.get_job(job_id).await.unwrap();
    assert!(job.is_some());
    assert_eq!(job.unwrap().station_id, station_id);

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_claim_next_job_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_CLAIM_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create a job
    let _job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();

    // Claim using transaction
    let mut tx = pool.begin().await.unwrap();
    let claimed = repo.claim_next_job_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    assert!(claimed.is_some());
    let job = claimed.unwrap();
    assert_eq!(job.status, JobStatus::InProgress);

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_mark_completed_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_COMPLETE_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create and claim a job
    let job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();
    repo.claim_next_job().await.unwrap();

    // Mark completed using transaction
    let stats = ImportStats {
        readings_imported: 100,
        start_date: None,
        end_date: None,
        duration_secs: 1.5,
    };

    let mut tx = pool.begin().await.unwrap();
    repo.mark_completed_tx(&mut tx, job_id, &stats)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Verify status
    let job = repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Completed);

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_mark_failed_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_FAILED_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create and claim a job
    let job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();
    repo.claim_next_job().await.unwrap();

    // Mark failed using transaction
    let error_entry = ErrorHistoryEntry {
        timestamp: Utc::now(),
        error: "Test error".to_string(),
        retry_count: 1,
    };
    let next_retry = Utc::now() + chrono::Duration::minutes(5);

    let mut tx = pool.begin().await.unwrap();
    repo.mark_failed_tx(&mut tx, job_id, "Test error", &error_entry, 1, next_retry)
        .await
        .unwrap();
    tx.commit().await.unwrap();

    // Verify status
    let job = repo.get_job(job_id).await.unwrap().unwrap();
    assert_eq!(job.status, JobStatus::Failed);
    assert!(job.error_message.is_some());

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_job_exists_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_EXISTS_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create a job
    repo.create_job(station_id, "test", 1, None).await.unwrap();

    // Check existence using transaction
    let mut tx = pool.begin().await.unwrap();
    let exists = repo.job_exists_tx(&mut tx, station_id).await.unwrap();
    tx.commit().await.unwrap();

    assert!(exists);

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_get_job_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_GET_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create a job
    let job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();

    // Get job using transaction
    let mut tx = pool.begin().await.unwrap();
    let job = repo.get_job_tx(&mut tx, job_id).await.unwrap();
    tx.commit().await.unwrap();

    assert!(job.is_some());
    assert_eq!(job.unwrap().id, job_id);

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[tokio::test]
#[serial]
async fn test_transaction_methods_get_pending_jobs_tx() {
    let pool = fopr_job_repo_fixtures::setup_test_db().await;
    let repo = FoprImportJobRepository::new(pool.clone());
    let station_id = "TX_PENDING_001";

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;

    // Create a pending job
    let job_id = repo.create_job(station_id, "test", 1, None).await.unwrap();

    // Get pending jobs using transaction
    let mut tx = pool.begin().await.unwrap();
    let jobs = repo.get_pending_jobs_tx(&mut tx).await.unwrap();
    tx.commit().await.unwrap();

    assert!(jobs.iter().any(|j| j.id == job_id));

    fopr_job_repo_fixtures::cleanup_jobs(&pool, station_id).await;
}

#[test]
fn test_job_status_serialization() {
    // Test that JobStatus serializes/deserializes correctly
    let pending = JobStatus::Pending;
    let json = serde_json::to_string(&pending).unwrap();
    assert_eq!(json, "\"pending\"");

    let in_progress = JobStatus::InProgress;
    let json = serde_json::to_string(&in_progress).unwrap();
    assert_eq!(json, "\"inprogress\"");

    let completed = JobStatus::Completed;
    let json = serde_json::to_string(&completed).unwrap();
    assert_eq!(json, "\"completed\"");

    let failed = JobStatus::Failed;
    let json = serde_json::to_string(&failed).unwrap();
    assert_eq!(json, "\"failed\"");
}

#[test]
fn test_job_status_deserialization() {
    // Test deserialization
    let status: JobStatus = serde_json::from_str("\"pending\"").unwrap();
    assert_eq!(status, JobStatus::Pending);

    let status: JobStatus = serde_json::from_str("\"inprogress\"").unwrap();
    assert_eq!(status, JobStatus::InProgress);

    let status: JobStatus = serde_json::from_str("\"completed\"").unwrap();
    assert_eq!(status, JobStatus::Completed);

    let status: JobStatus = serde_json::from_str("\"failed\"").unwrap();
    assert_eq!(status, JobStatus::Failed);
}

#[test]
fn test_import_stats_serialization() {
    use rain_tracker_service::db::fopr_import_job_repository::ImportStats;

    let stats = ImportStats {
        readings_imported: 150,
        start_date: Some("2023-01-01".to_string()),
        end_date: Some("2023-12-31".to_string()),
        duration_secs: 45.2,
    };

    let json = serde_json::to_string(&stats).unwrap();
    assert!(json.contains("150"));
    assert!(json.contains("45.2"));

    // Verify round-trip
    let deserialized: ImportStats = serde_json::from_str(&json).unwrap();
    assert_eq!(deserialized.readings_imported, 150);
    assert_eq!(deserialized.duration_secs, 45.2);
}

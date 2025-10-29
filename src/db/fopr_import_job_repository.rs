use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tracing::{debug, info, instrument};

use crate::db::DbError;
use crate::gauge_list_fetcher::GaugeSummary as FetchedGauge;

/// Job status for FOPR imports
#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::Type, Serialize, Deserialize)]
#[sqlx(type_name = "text")]
#[serde(rename_all = "lowercase")]
pub enum JobStatus {
    #[sqlx(rename = "pending")]
    Pending,
    #[sqlx(rename = "in_progress")]
    InProgress,
    #[sqlx(rename = "completed")]
    Completed,
    #[sqlx(rename = "failed")]
    Failed,
}

/// FOPR import job from database
#[derive(Debug, Clone)]
pub struct FoprImportJob {
    pub id: i32,
    pub station_id: String,
    pub status: JobStatus,
    pub priority: i32,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub error_message: Option<String>,
    pub error_history: serde_json::Value,
    pub retry_count: i32,
    pub max_retries: i32,
    pub next_retry_at: Option<DateTime<Utc>>,
    pub source: String,
    pub gauge_summary: Option<serde_json::Value>,
    pub import_stats: Option<serde_json::Value>,
}

/// Error entry for error history
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorHistoryEntry {
    pub timestamp: DateTime<Utc>,
    pub error: String,
    pub retry_count: i32,
}

/// Import statistics after completion
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImportStats {
    pub readings_imported: i64,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub duration_secs: f64,
}

#[derive(Clone)]
pub struct FoprImportJobRepository {
    pool: PgPool,
}

impl FoprImportJobRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Create a new import job
    #[instrument(skip(self, gauge_summary), fields(station_id = %station_id))]
    pub async fn create_job(
        &self,
        station_id: &str,
        source: &str,
        priority: i32,
        gauge_summary: Option<&FetchedGauge>,
    ) -> Result<i32, DbError> {
        debug!("Creating FOPR import job for station {}", station_id);

        let gauge_summary_json = gauge_summary
            .map(|g| serde_json::to_value(g).unwrap())
            .unwrap_or(serde_json::Value::Null);

        let job_id = sqlx::query_scalar!(
            r#"
            INSERT INTO fopr_import_jobs (
                station_id, status, priority, source, gauge_summary
            )
            VALUES ($1, 'pending', $2, $3, $4)
            RETURNING id
            "#,
            station_id,
            priority,
            source,
            gauge_summary_json
        )
        .fetch_one(&self.pool)
        .await?;

        info!(
            "Created FOPR import job {} for station {}",
            job_id, station_id
        );
        Ok(job_id)
    }

    /// Atomically claim the next job to process
    ///
    /// This uses FOR UPDATE SKIP LOCKED to safely handle concurrent workers.
    /// Returns the next pending job or a failed job ready for retry.
    #[instrument(skip(self))]
    pub async fn claim_next_job(&self) -> Result<Option<FoprImportJob>, DbError> {
        debug!("Attempting to claim next job");

        let job = sqlx::query_as!(
            FoprImportJob,
            r#"
            UPDATE fopr_import_jobs
            SET status = 'in_progress',
                started_at = NOW()
            WHERE id = (
                SELECT id
                FROM fopr_import_jobs
                WHERE status = 'pending'
                   OR (status = 'failed' AND retry_count < max_retries AND next_retry_at <= NOW())
                ORDER BY priority DESC, created_at ASC
                LIMIT 1
                FOR UPDATE SKIP LOCKED
            )
            RETURNING
                id, station_id, status AS "status: JobStatus",
                priority, created_at, started_at, completed_at,
                error_message, error_history, retry_count, max_retries, next_retry_at,
                source, gauge_summary, import_stats
            "#,
        )
        .fetch_optional(&self.pool)
        .await?;

        if let Some(ref j) = job {
            info!("Claimed job {} for station {}", j.id, j.station_id);
        } else {
            debug!("No jobs available to claim");
        }

        Ok(job)
    }

    /// Mark a job as completed with statistics
    #[instrument(skip(self, stats), fields(job_id = job_id))]
    pub async fn mark_completed(&self, job_id: i32, stats: &ImportStats) -> Result<(), DbError> {
        debug!("Marking job {} as completed", job_id);

        let stats_json = serde_json::to_value(stats).unwrap();

        sqlx::query!(
            r#"
            UPDATE fopr_import_jobs
            SET status = 'completed',
                completed_at = NOW(),
                import_stats = $2,
                error_message = NULL
            WHERE id = $1
            "#,
            job_id,
            stats_json
        )
        .execute(&self.pool)
        .await?;

        info!("Job {} marked as completed", job_id);
        Ok(())
    }

    /// Mark a job as failed and schedule retry if applicable
    #[instrument(skip(self), fields(job_id = job_id, error = %error))]
    pub async fn mark_failed(
        &self,
        job_id: i32,
        error: &str,
        retry_count: i32,
    ) -> Result<(), DbError> {
        debug!("Marking job {} as failed (retry {})", job_id, retry_count);

        // Calculate next retry time with exponential backoff
        // 5 min, 15 min, 45 min
        let retry_delay_secs = match retry_count {
            0 => 5 * 60,  // 5 minutes
            1 => 15 * 60, // 15 minutes
            _ => 45 * 60, // 45 minutes
        };

        let next_retry_at = Utc::now() + chrono::Duration::seconds(retry_delay_secs);

        // Build error history entry
        let error_entry = ErrorHistoryEntry {
            timestamp: Utc::now(),
            error: error.to_string(),
            retry_count,
        };
        let error_entry_json = serde_json::to_value(&error_entry).unwrap();

        sqlx::query!(
            r#"
            UPDATE fopr_import_jobs
            SET status = 'failed',
                error_message = $2,
                error_history = error_history || $3::jsonb,
                retry_count = $4,
                next_retry_at = $5
            WHERE id = $1
            "#,
            job_id,
            error,
            error_entry_json,
            retry_count,
            next_retry_at
        )
        .execute(&self.pool)
        .await?;

        info!(
            "Job {} marked as failed, retry {} scheduled for {}",
            job_id, retry_count, next_retry_at
        );
        Ok(())
    }

    /// Check if a job already exists for a station
    #[instrument(skip(self), fields(station_id = %station_id))]
    pub async fn job_exists(&self, station_id: &str) -> Result<bool, DbError> {
        let exists = sqlx::query_scalar!(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM fopr_import_jobs
                WHERE station_id = $1
                  AND status IN ('pending', 'in_progress')
            )
            "#,
            station_id
        )
        .fetch_one(&self.pool)
        .await?
        .unwrap_or(false);

        debug!("Job exists check for station {}: {}", station_id, exists);
        Ok(exists)
    }

    /// Get job by ID
    #[instrument(skip(self), fields(job_id = job_id))]
    pub async fn get_job(&self, job_id: i32) -> Result<Option<FoprImportJob>, DbError> {
        let job = sqlx::query_as!(
            FoprImportJob,
            r#"
            SELECT
                id, station_id, status AS "status: JobStatus",
                priority, created_at, started_at, completed_at,
                error_message, error_history, retry_count, max_retries, next_retry_at,
                source, gauge_summary, import_stats
            FROM fopr_import_jobs
            WHERE id = $1
            "#,
            job_id
        )
        .fetch_optional(&self.pool)
        .await?;

        Ok(job)
    }

    /// Get all pending jobs (for monitoring/debugging)
    #[instrument(skip(self))]
    pub async fn get_pending_jobs(&self) -> Result<Vec<FoprImportJob>, DbError> {
        let jobs = sqlx::query_as!(
            FoprImportJob,
            r#"
            SELECT
                id, station_id, status AS "status: JobStatus",
                priority, created_at, started_at, completed_at,
                error_message, error_history, retry_count, max_retries, next_retry_at,
                source, gauge_summary, import_stats
            FROM fopr_import_jobs
            WHERE status IN ('pending', 'failed')
            ORDER BY priority DESC, created_at ASC
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        debug!("Found {} pending jobs", jobs.len());
        Ok(jobs)
    }
}

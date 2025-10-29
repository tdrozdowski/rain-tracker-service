-- Create FOPR import job queue table
--
-- This table tracks on-demand FOPR imports triggered by gauge discovery.
-- When the gauge scraper discovers a new gauge, it creates a job here.
-- Background workers claim jobs and perform the FOPR import.

CREATE TABLE fopr_import_jobs (
    id SERIAL PRIMARY KEY,
    station_id TEXT NOT NULL,
    status TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 10,

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    started_at TIMESTAMPTZ,
    completed_at TIMESTAMPTZ,

    -- Error tracking
    error_message TEXT,                    -- Most recent error
    error_history JSONB DEFAULT '[]'::jsonb,  -- Full error history with timestamps

    -- Retry logic
    retry_count INTEGER NOT NULL DEFAULT 0,
    max_retries INTEGER NOT NULL DEFAULT 3,
    next_retry_at TIMESTAMPTZ,

    -- Job metadata
    source TEXT NOT NULL DEFAULT 'gauge_discovery',  -- 'gauge_discovery', 'manual', 'backfill'
    gauge_summary JSONB,                   -- Store FetchedGauge from scraper
    import_stats JSONB,                    -- Completion stats: {readings_imported, start_date, end_date, etc}

    -- Constraints
    CONSTRAINT valid_status CHECK (status IN ('pending', 'in_progress', 'completed', 'failed')),
    CONSTRAINT valid_priority CHECK (priority >= 0 AND priority <= 100),
    CONSTRAINT retry_count_check CHECK (retry_count >= 0 AND retry_count <= max_retries)
);

-- Indexes for job queue queries
CREATE INDEX idx_fopr_jobs_status ON fopr_import_jobs(status);
CREATE INDEX idx_fopr_jobs_next_retry ON fopr_import_jobs(next_retry_at) WHERE status = 'failed';
CREATE INDEX idx_fopr_jobs_priority ON fopr_import_jobs(priority DESC, created_at ASC) WHERE status IN ('pending', 'failed');
CREATE INDEX idx_fopr_jobs_station_id ON fopr_import_jobs(station_id);

-- Comments
COMMENT ON TABLE fopr_import_jobs IS 'Job queue for on-demand FOPR (Full Operational Period of Record) imports';
COMMENT ON COLUMN fopr_import_jobs.status IS 'Job status: pending, in_progress, completed, failed';
COMMENT ON COLUMN fopr_import_jobs.priority IS 'Job priority (0-100, higher = more urgent). Default: 10';
COMMENT ON COLUMN fopr_import_jobs.error_history IS 'Array of error objects: [{timestamp, error, retry_count}, ...]';
COMMENT ON COLUMN fopr_import_jobs.gauge_summary IS 'Serialized FetchedGauge from scraper to avoid waiting 60min for next scrape';
COMMENT ON COLUMN fopr_import_jobs.import_stats IS 'Import completion stats: {readings_imported, start_date, end_date, duration_secs}';

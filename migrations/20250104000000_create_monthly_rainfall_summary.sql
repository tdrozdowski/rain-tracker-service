-- Create monthly_rainfall_summary table for pre-aggregated monthly totals
-- This table stores monthly rainfall totals per gauge, making calendar/water year queries trivial

CREATE TABLE IF NOT EXISTS monthly_rainfall_summary (
    id BIGSERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL,
    year INT NOT NULL,
    month INT NOT NULL CHECK (month >= 1 AND month <= 12),
    total_rainfall_inches DOUBLE PRECISION NOT NULL DEFAULT 0.0,
    reading_count INT NOT NULL DEFAULT 0,
    first_reading_date TIMESTAMPTZ,
    last_reading_date TIMESTAMPTZ,
    min_cumulative_inches DOUBLE PRECISION,
    max_cumulative_inches DOUBLE PRECISION,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(station_id, year, month)
);

-- Indexes for common query patterns
CREATE INDEX idx_monthly_summary_station_year ON monthly_rainfall_summary(station_id, year);
CREATE INDEX idx_monthly_summary_station_year_month ON monthly_rainfall_summary(station_id, year, month);

-- Comments for documentation
COMMENT ON TABLE monthly_rainfall_summary IS 'Pre-aggregated monthly rainfall totals per gauge. Updated when new readings are inserted.';
COMMENT ON COLUMN monthly_rainfall_summary.total_rainfall_inches IS 'Total rainfall for this month (calculated from incremental values or cumulative differences)';
COMMENT ON COLUMN monthly_rainfall_summary.min_cumulative_inches IS 'Minimum cumulative value seen in this month (usually first reading)';
COMMENT ON COLUMN monthly_rainfall_summary.max_cumulative_inches IS 'Maximum cumulative value seen in this month (usually last reading)';

-- Create rain_readings table
CREATE TABLE IF NOT EXISTS rain_readings (
    id BIGSERIAL PRIMARY KEY,
    reading_datetime TIMESTAMPTZ NOT NULL,
    cumulative_inches NUMERIC(6, 2) NOT NULL,
    incremental_inches NUMERIC(6, 2) NOT NULL,
    station_id VARCHAR(20) NOT NULL DEFAULT '59700',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(reading_datetime, station_id)
);

-- Index for common queries
CREATE INDEX idx_reading_datetime ON rain_readings(reading_datetime DESC);
CREATE INDEX idx_station_datetime ON rain_readings(station_id, reading_datetime DESC);

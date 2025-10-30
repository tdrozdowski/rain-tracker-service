-- Widen station_id from VARCHAR(20) to VARCHAR(50)
--
-- Issue: Some MCFCD station IDs (particularly partnership gauges) exceed 20 characters
-- Error: "value too long for type character varying(20)"
-- Solution: Increase to VARCHAR(50) to accommodate all known station ID formats

-- 1. rain_readings table
ALTER TABLE rain_readings
    ALTER COLUMN station_id TYPE VARCHAR(50);

-- 2. gauge_summaries table
ALTER TABLE gauge_summaries
    ALTER COLUMN station_id TYPE VARCHAR(50);

-- 3. gauges table (primary key)
ALTER TABLE gauges
    ALTER COLUMN station_id TYPE VARCHAR(50);

-- 4. monthly_rainfall_summary table
ALTER TABLE monthly_rainfall_summary
    ALTER COLUMN station_id TYPE VARCHAR(50);

-- Comments
COMMENT ON COLUMN rain_readings.station_id IS 'MCFCD station ID (up to 50 chars for partnership gauges)';
COMMENT ON COLUMN gauge_summaries.station_id IS 'MCFCD station ID (up to 50 chars for partnership gauges)';
COMMENT ON COLUMN gauges.station_id IS 'MCFCD station ID (primary key, up to 50 chars for partnership gauges)';
COMMENT ON COLUMN monthly_rainfall_summary.station_id IS 'MCFCD station ID (up to 50 chars for partnership gauges)';

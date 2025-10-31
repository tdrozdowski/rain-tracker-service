-- Add foreign key constraints to link existing tables to the gauges table
--
-- IMPORTANT: This migration should ONLY be run AFTER:
-- 1. The gauges table has been populated via FOPR import
-- 2. All station_ids in rain_readings, gauge_summaries, and monthly_rainfall_summary
--    exist in the gauges table
--
-- To verify before running:
-- SELECT DISTINCT r.station_id
-- FROM rain_readings r
-- LEFT JOIN gauges g ON r.station_id = g.station_id
-- WHERE g.station_id IS NULL;
-- (Should return 0 rows)

-- Add foreign key from rain_readings to gauges
ALTER TABLE rain_readings
    ADD CONSTRAINT fk_rain_readings_gauge
    FOREIGN KEY (station_id) REFERENCES gauges(station_id)
    ON DELETE RESTRICT;  -- Don't allow deleting gauge if readings exist

-- Add foreign key from gauge_summaries to gauges
ALTER TABLE gauge_summaries
    ADD CONSTRAINT fk_gauge_summaries_gauge
    FOREIGN KEY (station_id) REFERENCES gauges(station_id)
    ON DELETE CASCADE;  -- Delete summary if gauge deleted

-- Add foreign key from monthly_rainfall_summary to gauges
ALTER TABLE monthly_rainfall_summary
    ADD CONSTRAINT fk_monthly_summary_gauge
    FOREIGN KEY (station_id) REFERENCES gauges(station_id)
    ON DELETE CASCADE;  -- Delete summaries if gauge deleted

COMMENT ON CONSTRAINT fk_rain_readings_gauge ON rain_readings
    IS 'Ensures all readings reference a valid gauge. RESTRICT prevents orphaned readings.';

COMMENT ON CONSTRAINT fk_gauge_summaries_gauge ON gauge_summaries
    IS 'Ensures all summaries reference a valid gauge. CASCADE deletes summaries if gauge deleted.';

COMMENT ON CONSTRAINT fk_monthly_summary_gauge ON monthly_rainfall_summary
    IS 'Ensures all monthly summaries reference a valid gauge. CASCADE deletes summaries if gauge deleted.';

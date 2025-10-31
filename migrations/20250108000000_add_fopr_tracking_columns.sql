-- Add FOPR import tracking columns to gauges table
--
-- These columns track the availability and import status of FOPR (Full Operational Period of Record) files
-- for each gauge. This enables:
-- 1. Identifying gauges without FOPR files (404 responses)
-- 2. Tracking when FOPR data was last imported
-- 3. Detecting stale imports that need refreshing
-- 4. Implementing scheduled annual re-import after water year updates

-- Add FOPR availability flag
-- FALSE indicates the FOPR file returned 404 (gauge exists but no historical data available)
-- TRUE indicates FOPR file was successfully downloaded (default assumption)
ALTER TABLE gauges
    ADD COLUMN fopr_available BOOLEAN DEFAULT TRUE;

-- Add last import timestamp
-- Tracks when FOPR data was last successfully imported for this gauge
-- NULL indicates never imported (either new gauge or FOPR not available)
ALTER TABLE gauges
    ADD COLUMN fopr_last_import_date DATE;

-- Add last check timestamp
-- Tracks when we last attempted to download/check the FOPR file
-- Used to identify gauges that haven't been checked recently
ALTER TABLE gauges
    ADD COLUMN fopr_last_checked_date DATE;

-- Create index for querying stale imports
-- Enables efficient queries for gauges needing re-import:
-- WHERE fopr_available = TRUE AND (fopr_last_import_date IS NULL OR fopr_last_import_date < NOW() - INTERVAL '1 year')
CREATE INDEX IF NOT EXISTS idx_gauges_fopr_import_status
    ON gauges(fopr_available, fopr_last_import_date)
    WHERE fopr_available = TRUE;

-- Create index for gauges without FOPR
-- Enables efficient queries for gauges known to lack FOPR files
CREATE INDEX IF NOT EXISTS idx_gauges_no_fopr
    ON gauges(station_id)
    WHERE fopr_available = FALSE;

-- Comments for documentation
COMMENT ON COLUMN gauges.fopr_available IS 'FALSE if FOPR file returned 404; TRUE if available or unknown';
COMMENT ON COLUMN gauges.fopr_last_import_date IS 'When FOPR data was last successfully imported (NULL = never imported)';
COMMENT ON COLUMN gauges.fopr_last_checked_date IS 'When we last attempted to fetch the FOPR file';

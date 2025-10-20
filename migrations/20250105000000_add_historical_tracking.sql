-- Add columns for tracking data source and import metadata
ALTER TABLE rain_readings
ADD COLUMN IF NOT EXISTS data_source VARCHAR(50) NOT NULL DEFAULT 'live_scrape',
ADD COLUMN IF NOT EXISTS import_metadata JSONB;

-- Index for filtering by source
CREATE INDEX IF NOT EXISTS idx_rain_readings_data_source ON rain_readings(data_source);

-- Comments for documentation
COMMENT ON COLUMN rain_readings.data_source IS 'Source of the data: live_scrape, pdf_MMYY, excel_WY_YYYY';
COMMENT ON COLUMN rain_readings.import_metadata IS 'JSON metadata about the import (footnotes, estimated values, outage info)';

-- Example data_source values:
-- 'live_scrape'     - Current real-time scraping (default)
-- 'pdf_1119'        - PDF import for November 2019
-- 'excel_WY_2023'   - Excel import for Water Year 2023

-- Example import_metadata JSONB:
-- {"footnote": "Gage down due to battery failure", "estimated": true}
-- {"outage_start": "2019-11-13T06:00:00", "outage_end": "2019-11-16T12:00:00"}

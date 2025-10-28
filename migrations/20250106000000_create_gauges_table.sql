-- Create gauges reference table
-- This table contains static metadata about each rain gauge
-- Data populated primarily from FOPR Meta_Stats sheets

CREATE TABLE IF NOT EXISTS gauges (
    -- Primary key: MCFCD station ID (no autoincrement)
    station_id VARCHAR(20) PRIMARY KEY,

    -- Identification
    station_name VARCHAR(255),                -- "Aztec Park"
    station_type VARCHAR(50) DEFAULT 'Rain',  -- "Rain", "Stream", etc.
    previous_station_ids TEXT[],              -- ["4695"] - for data reconciliation

    -- Location
    latitude DECIMAL(10, 7),                  -- 33.61006 (decimal degrees)
    longitude DECIMAL(10, 7),                 -- -111.86545 (decimal degrees)
    elevation_ft INTEGER,                     -- 1465
    county VARCHAR(100) DEFAULT 'Maricopa',
    city VARCHAR(100),                        -- "Scottsdale"
    location_description TEXT,                -- "Near Thunderbird & Frank Lloyd Wright"

    -- Operational metadata
    installation_date DATE,                   -- Calculated from "years since installation"
    data_begins_date DATE,                    -- Parsed from Excel date
    data_ends_date DATE,                      -- NULL if still active
    status VARCHAR(50) DEFAULT 'Active',      -- "Active", "Inactive", "Decommissioned"

    -- Climate statistics (from FOPR)
    avg_annual_precipitation_inches DECIMAL(6, 2),  -- 7.48
    complete_years_count INTEGER,                   -- 26

    -- Data quality
    incomplete_months_count INTEGER DEFAULT 0,
    missing_months_count INTEGER DEFAULT 0,
    data_quality_remarks TEXT,                      -- "Records Good"

    -- Additional FOPR metadata as JSONB
    fopr_metadata JSONB,

    -- Tracking
    metadata_source VARCHAR(100) DEFAULT 'fopr_import',
    metadata_updated_at TIMESTAMPTZ DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- Spatial index for lat/long queries (nearest gauge, distance calculations)
-- Note: Requires earthdistance extension. Create manually if needed:
--   CREATE EXTENSION IF NOT EXISTS cube;
--   CREATE EXTENSION IF NOT EXISTS earthdistance;
--   CREATE INDEX idx_gauges_location ON gauges USING GIST(ll_to_earth(latitude, longitude, 0));
-- For now, using simple lat/lon indexes instead:
CREATE INDEX IF NOT EXISTS idx_gauges_latitude ON gauges(latitude);
CREATE INDEX IF NOT EXISTS idx_gauges_longitude ON gauges(longitude);

-- Indexes for common queries
CREATE INDEX IF NOT EXISTS idx_gauges_city ON gauges(city);
CREATE INDEX IF NOT EXISTS idx_gauges_county ON gauges(county);
CREATE INDEX IF NOT EXISTS idx_gauges_status ON gauges(status);
CREATE INDEX IF NOT EXISTS idx_gauges_station_type ON gauges(station_type);

-- Comments
COMMENT ON TABLE gauges IS 'Static reference data for rain gauges. Metadata populated from FOPR files.';
COMMENT ON COLUMN gauges.station_id IS 'MCFCD station ID (primary key, no autoincrement)';
COMMENT ON COLUMN gauges.previous_station_ids IS 'Array of historical station IDs for this gauge';
COMMENT ON COLUMN gauges.fopr_metadata IS 'JSONB: frequency statistics, storm counts, etc.';

-- NOTE: Foreign key constraints from other tables (rain_readings, gauge_summaries, monthly_rainfall_summary)
-- will be added in a later migration AFTER FOPR data has been imported to populate this table.
-- See migration: 20250107000000_add_gauge_foreign_keys.sql (to be created after FOPR import)

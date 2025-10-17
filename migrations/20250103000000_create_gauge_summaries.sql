-- Create gauge_summaries table for storing aggregate gauge information
CREATE TABLE IF NOT EXISTS gauge_summaries (
    id BIGSERIAL PRIMARY KEY,
    station_id VARCHAR(20) NOT NULL UNIQUE,
    gauge_name VARCHAR(255) NOT NULL,
    city_town VARCHAR(255),
    elevation_ft INTEGER,
    general_location TEXT,
    msp_forecast_zone VARCHAR(100),

    -- Recent rainfall data from the summary file
    rainfall_past_6h_inches DOUBLE PRECISION,
    rainfall_past_24h_inches DOUBLE PRECISION,

    -- Metadata
    last_scraped_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_gauge_station_id ON gauge_summaries(station_id);
CREATE INDEX idx_gauge_city_town ON gauge_summaries(city_town);
CREATE INDEX idx_gauge_last_scraped ON gauge_summaries(last_scraped_at DESC);

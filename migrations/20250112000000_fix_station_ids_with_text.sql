-- Fix station_ids that have additional text appended (e.g., "29200 since 03/09/18")
-- Station IDs should be exactly 5 digits, no text

-- Update gauges table: extract first 5 digits from station_id
UPDATE gauges
SET station_id = SUBSTRING(station_id FROM '^\d{5}')
WHERE station_id ~ '^\d{5}\s';  -- Match station_ids that have 5 digits followed by whitespace

-- Update rain_readings table: extract first 5 digits from station_id
UPDATE rain_readings
SET station_id = SUBSTRING(station_id FROM '^\d{5}')
WHERE station_id ~ '^\d{5}\s';  -- Match station_ids that have 5 digits followed by whitespace

-- Update monthly_rainfall_summary table: extract first 5 digits from station_id
UPDATE monthly_rainfall_summary
SET station_id = SUBSTRING(station_id FROM '^\d{5}')
WHERE station_id ~ '^\d{5}\s';  -- Match station_ids that have 5 digits followed by whitespace

-- Note: This migration handles cases where gauge IDs were imported with text like:
-- "29200 since 03/09/18" -> "29200"
-- The regex '^\d{5}\s' matches station_ids starting with exactly 5 digits followed by space
-- The SUBSTRING extracts just the first 5 digits

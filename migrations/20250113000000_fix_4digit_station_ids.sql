-- Fix station_ids that have additional text appended for 4-digit IDs (e.g., "1800 since 03/27/18")
-- Station IDs can be either 4 or 5 digits, no additional text

-- Update gauges table: extract first 4 digits from station_id where pattern is 4 digits + whitespace
UPDATE gauges
SET station_id = SUBSTRING(station_id FROM '^\d{4}')
WHERE station_id ~ '^\d{4}\s';  -- Match station_ids that have 4 digits followed by whitespace

-- Update rain_readings table: extract first 4 digits from station_id
UPDATE rain_readings
SET station_id = SUBSTRING(station_id FROM '^\d{4}')
WHERE station_id ~ '^\d{4}\s';  -- Match station_ids that have 4 digits followed by whitespace

-- Update monthly_rainfall_summary table: extract first 4 digits from station_id
UPDATE monthly_rainfall_summary
SET station_id = SUBSTRING(station_id FROM '^\d{4}')
WHERE station_id ~ '^\d{4}\s';  -- Match station_ids that have 4 digits followed by whitespace

-- Note: This migration handles cases where 4-digit gauge IDs were imported with text like:
-- "1800 since 03/27/18" -> "1800"
-- The regex '^\d{4}\s' matches station_ids starting with exactly 4 digits followed by space
-- The SUBSTRING extracts just the first 4 digits
-- This complements migration 20250112000000 which handled 5-digit IDs

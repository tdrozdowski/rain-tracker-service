-- Change cumulative_inches and incremental_inches from NUMERIC to DOUBLE PRECISION
ALTER TABLE rain_readings
    ALTER COLUMN cumulative_inches TYPE DOUBLE PRECISION,
    ALTER COLUMN incremental_inches TYPE DOUBLE PRECISION;

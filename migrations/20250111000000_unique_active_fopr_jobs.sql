-- Add partial unique index to prevent duplicate active FOPR import jobs
--
-- Issue: Multiple jobs can be created for the same station_id, leading to:
--   - Race conditions during gauge discovery
--   - Wasted worker resources processing duplicate imports
--   - Potential data inconsistencies
--
-- Solution: Partial unique index on station_id for active jobs only
--   - Prevents duplicate 'pending' or 'in_progress' jobs for same station
--   - Allows new job creation after permanent failure (status = 'failed')
--   - Preserves audit trail (multiple 'completed' jobs allowed)
--
-- This is a standard PostgreSQL feature (available since v7.2, 2002)

CREATE UNIQUE INDEX unique_active_fopr_import_jobs
ON fopr_import_jobs(station_id)
WHERE status IN ('pending', 'in_progress');

-- Comments
COMMENT ON INDEX unique_active_fopr_import_jobs IS
'Ensures only one active (pending/in_progress) FOPR import job exists per station.
Allows retries after permanent failure and preserves audit history.';

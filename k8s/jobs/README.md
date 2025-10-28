# Kubernetes Jobs for Historical Data Import

This directory contains Kubernetes job manifests for importing historical rainfall data from the Maricopa County Flood Control District (MCFCD) into the Rain Tracker database.

## Overview

The Rain Tracker service has two data collection systems:
1. **Live scraping**: Continuous scraping of current rainfall data (main service)
2. **Historical import**: One-time or periodic imports of historical data (these jobs)

These K8s jobs handle the historical import use cases with different levels of automation and scheduling.

---

## Directory Structure

This directory uses **Kustomize** for managing configurations across environments:

```
k8s/jobs/
├── base/                           # Base configuration (all environments)
│   ├── kustomization.yaml          # Kustomize base config
│   ├── import-job-config.yaml      # ConfigMap, Secrets, RBAC
│   ├── historical-import-cronjob.yaml  # CronJob definitions
│   └── *-import.yaml               # Job templates (use via scripts)
├── overlays/
│   ├── dev/                        # Development environment
│   ├── staging/                    # Staging environment
│   └── production/                 # Production environment
├── README.md                       # This file (detailed usage)
└── KUSTOMIZE.md                    # Kustomize-specific guide
```

**📘 For Kustomize usage**, see [KUSTOMIZE.md](KUSTOMIZE.md)

**📘 For job usage**, continue reading this file

---

## Quick Start

### Using Kustomize (Recommended)

```bash
# Deploy to production with Kustomize
kubectl apply -k k8s/jobs/overlays/production

# Verify
kubectl get configmap historical-import-config -n rain-tracker
kubectl get cronjobs -n rain-tracker
```

### Using Direct Manifests

```bash
# Apply import job configuration
kubectl apply -f k8s/jobs/base/import-job-config.yaml

# Verify prerequisites
kubectl get secret db-secrets -n rain-tracker
kubectl get configmap historical-import-config -n rain-tracker
```

### Import a Single Water Year

The most common use case - import one specific water year:

```bash
# Option 1: Use the helper script (recommended)
./scripts/import-water-year.sh 2023

# Option 2: Apply the manifest directly
# Edit the WATER_YEAR value in historical-single-year-import.yaml first
kubectl create -f k8s/jobs/historical-single-year-import.yaml

# Option 3: One-liner with sed
cat k8s/jobs/historical-single-year-import.yaml | \
  sed 's/value: "2023"/value: "2024"/' | \
  kubectl create -f -
```

**Monitor progress**:
```bash
# Watch job status
kubectl get jobs -n rain-tracker -l job-type=historical-single-year

# Follow logs
kubectl logs -f -n rain-tracker -l job-type=historical-single-year

# Check if job completed successfully
kubectl get jobs -n rain-tracker -l job-type=historical-single-year
```

---

## Available Job Types

### 1. Single Year Import (`historical-single-year-import.yaml`)

**Purpose**: Import data for one specific water year

**When to use**:
- Initial historical data load
- Backfilling missing years
- Updating a specific year after corrections
- One-off imports

**Configuration**:
```yaml
env:
  - name: WATER_YEAR
    value: "2023"  # ← Change this
```

**Resources**: 256Mi-1Gi RAM, 250m-1000m CPU

**Duration**: ~2-3 minutes per year (350+ gauges × 365 days)

**Example**:
```bash
# Import water year 2023 (Oct 1, 2022 - Sep 30, 2023)
./scripts/import-water-year.sh 2023

# Import current water year
CURRENT_WY=$(date +%Y)
if [ $(date +%m) -ge 10 ]; then
  CURRENT_WY=$((CURRENT_WY + 1))
fi
./scripts/import-water-year.sh $CURRENT_WY
```

---

### 2. Bulk Import (`historical-bulk-import.yaml`)

**Purpose**: Import a range of water years in sequence

**When to use**:
- Initial database population (import all available years)
- Backfilling large date ranges
- Annual bulk updates

**Configuration**:
```yaml
env:
  - name: START_YEAR
    value: "2010"  # ← First year to import
  - name: END_YEAR
    value: "2024"  # ← Last year to import (inclusive)
```

**Two variants provided**:

#### Variant 1: Simple (single year at a time)
```bash
kubectl create -f k8s/jobs/historical-bulk-import.yaml
```

#### Variant 2: Sequential (bash script loops through years)
```bash
# Edit START_YEAR and END_YEAR in the manifest
kubectl create -f k8s/jobs/historical-bulk-import.yaml

# Example: Import all Excel-format years (2022-2024)
cat k8s/jobs/historical-bulk-import.yaml | \
  sed 's/value: "2010"/value: "2022"/' | \
  sed 's/value: "2024"/value: "2024"/' | \
  kubectl create -f -
```

**Resources**: 512Mi-2Gi RAM, 500m-2000m CPU

**Duration**: ~2-3 minutes per year × number of years
- Example: 2010-2024 (15 years) = ~30-45 minutes

**Error Handling**:
- Sequential variant continues on individual year failures
- Failed years are logged but don't stop the entire bulk import

---

### 3. FOPR Metadata Import (`fopr-metadata-import.yaml`)

**Purpose**: Import gauge metadata from FOPR (Full Operational Period of Record) files

**When to use**:
- Initial gauge metadata population
- Refreshing gauge location/statistics data
- Checking FOPR availability for all gauges

**Two variants provided**:

#### Variant 1: All Gauges
Processes all active gauges in the database:

```bash
kubectl create -f k8s/jobs/fopr-metadata-import.yaml

# Monitor progress
kubectl logs -f -n rain-tracker -l job-type=fopr-metadata-import
```

**What it does**:
1. Queries database for all active gauges
2. Downloads FOPR file for each gauge (if available)
3. Updates `gauges` table with availability status
4. Sets `fopr_available`, `fopr_last_checked_date`

**Resources**: 256Mi-512Mi RAM, 250m-500m CPU

**Duration**: ~2-3 seconds per gauge + download time
- Example: 350 gauges × 2 sec = ~12 minutes

**Rate Limiting**: 2-second delay between downloads to avoid overwhelming MCFCD server

#### Variant 2: Single Gauge
Process one specific gauge (useful for testing or one-off updates):

```bash
# Edit STATION_ID in the manifest
kubectl create -f k8s/jobs/fopr-metadata-import.yaml

# Or use sed for one-liner
cat k8s/jobs/fopr-metadata-import.yaml | \
  sed 's/value: "59700"/value: "12345"/' | \
  kubectl create -f -
```

**Note**: FOPR parsing extracts metadata only. Full daily rainfall data import from FOPR files is not yet implemented (see `docs/implementation-task-breakdown.md` R4).

---

### 4. CronJobs (`historical-import-cronjob.yaml`)

**Purpose**: Automated scheduled imports to keep data up-to-date

**Three CronJob variants provided**:

#### 4a. Daily Current Year Import
**Schedule**: Daily at 3:00 AM UTC

**Purpose**: Keep current water year data fresh as new daily readings are published

```bash
kubectl apply -f k8s/jobs/historical-import-cronjob.yaml

# Check CronJob schedule
kubectl get cronjobs -n rain-tracker

# View recent job executions
kubectl get jobs -n rain-tracker -l job-type=historical-import-cron

# Check next scheduled run
kubectl get cronjob historical-import-current-year -n rain-tracker
```

**What it does**:
1. Calculates current water year based on date
2. Imports that water year (updates existing data with ON CONFLICT)
3. Logs completion

**When to use**:
- After initial data load is complete
- To automatically stay current with latest data
- For ongoing maintenance

#### 4b. Weekly Recent Years Import
**Schedule**: Weekly on Sundays at 4:00 AM UTC

**Purpose**: Import current + previous year to catch late corrections

```bash
kubectl apply -f k8s/jobs/historical-import-cronjob.yaml
```

**What it does**:
1. Imports previous water year (might have retroactive corrections)
2. Waits 5 seconds
3. Imports current water year

**When to use**:
- When MCFCD publishes corrections to previous year's data
- For higher reliability (weekly vs daily)

#### 4c. Monthly FOPR Metadata Refresh
**Schedule**: Monthly on the 1st at 2:00 AM UTC

**Purpose**: Check for new gauges and update FOPR availability

```bash
kubectl apply -f k8s/jobs/historical-import-cronjob.yaml
```

**What it does**:
1. Finds gauges not checked in last 30 days
2. Checks FOPR availability for each
3. Updates `fopr_available` and `fopr_last_checked_date`

**When to use**:
- To discover newly installed gauges
- To keep metadata fresh

---

## Configuration Management

### ConfigMap (`import-job-config.yaml`)

Centralized configuration for all import jobs:

```bash
# Apply configuration
kubectl apply -f k8s/jobs/import-job-config.yaml

# View current configuration
kubectl get configmap historical-import-config -n rain-tracker -o yaml
```

**Key configuration values**:
```yaml
MCFCD_BASE_URL: "https://alert.fcd.maricopa.gov/alert/Rain/"
RUST_LOG: "info"
BATCH_SIZE: "1000"
SKIP_EXISTING_DATA: "true"
RECALCULATE_SUMMARIES: "true"
```

**Using the ConfigMap in jobs**:

Add to job spec:
```yaml
envFrom:
  - configMapRef:
      name: historical-import-config
```

This pulls all values from the ConfigMap instead of hardcoding them.

### Secrets

Database credentials are stored in Kubernetes secrets:

```bash
# Check existing secrets
kubectl get secret db-secrets -n rain-tracker

# View secret structure (base64 encoded)
kubectl get secret db-secrets -n rain-tracker -o yaml

# Decode DATABASE_URL
kubectl get secret db-secrets -n rain-tracker -o jsonpath='{.data.DATABASE_URL}' | base64 -d
```

**For production**: Use SealedSecrets, Vault, or cloud provider secret management instead of plain K8s secrets.

---

## Deployment Workflows

### Initial Historical Data Load

**Goal**: Import all available historical data for the first time

**Steps**:

1. **Apply configuration**:
   ```bash
   kubectl apply -f k8s/jobs/import-job-config.yaml
   ```

2. **Import all years (bulk)**:
   ```bash
   # Edit START_YEAR and END_YEAR in manifest
   # Example: 2010-2024 for all available data
   kubectl create -f k8s/jobs/historical-bulk-import.yaml
   ```

3. **Monitor progress**:
   ```bash
   # Watch logs in real-time
   kubectl logs -f -n rain-tracker -l job-type=historical-bulk-import-sequential

   # Check job status
   kubectl get jobs -n rain-tracker
   ```

4. **Import FOPR metadata** (optional but recommended):
   ```bash
   kubectl create -f k8s/jobs/fopr-metadata-import.yaml
   ```

5. **Set up automated updates**:
   ```bash
   # Enable daily current year imports
   kubectl apply -f k8s/jobs/historical-import-cronjob.yaml
   ```

**Expected duration**:
- 2010-2024 bulk import: ~30-45 minutes
- FOPR metadata: ~12 minutes (350 gauges)
- Total: ~45-60 minutes

### Backfilling Missing Years

**Goal**: Import specific missing years after initial load

**Steps**:

```bash
# Import each missing year
for year in 2015 2016 2019; do
  ./scripts/import-water-year.sh $year
  sleep 10  # Small delay between years
done

# Or edit bulk import manifest for just the missing range
```

### Updating Current Year

**Goal**: Refresh current water year data with latest readings

**One-time update**:
```bash
# Calculate current water year
CURRENT_WY=$(date +%Y)
if [ $(date +%m) -ge 10 ]; then
  CURRENT_WY=$((CURRENT_WY + 1))
fi

# Import
./scripts/import-water-year.sh $CURRENT_WY
```

**Automated updates**:
```bash
# Set up daily CronJob
kubectl apply -f k8s/jobs/historical-import-cronjob.yaml

# Verify it's scheduled
kubectl get cronjob historical-import-current-year -n rain-tracker
```

### Correcting Data Quality Issues

**Goal**: Re-import a year after MCFCD publishes corrections

**Steps**:

1. **Delete existing data** (optional, or rely on ON CONFLICT):
   ```bash
   # Connect to database
   kubectl exec -it <postgres-pod> -n rain-tracker -- psql -U rain_tracker

   # Delete readings for specific year and source
   DELETE FROM rain_readings
   WHERE data_source = 'excel_WY_2023'
     AND reading_date >= '2022-10-01'
     AND reading_date < '2023-10-01';
   ```

2. **Re-import the year**:
   ```bash
   ./scripts/import-water-year.sh 2023
   ```

3. **Verify import**:
   ```bash
   # Check reading counts
   psql -c "SELECT data_source, COUNT(*) FROM rain_readings
            WHERE reading_date >= '2022-10-01'
              AND reading_date < '2023-10-01'
            GROUP BY data_source;"
   ```

---

## Monitoring & Troubleshooting

### Check Job Status

```bash
# List all import jobs
kubectl get jobs -n rain-tracker -l app=rain-tracker

# List jobs by type
kubectl get jobs -n rain-tracker -l job-type=historical-single-year
kubectl get jobs -n rain-tracker -l job-type=historical-bulk-import
kubectl get jobs -n rain-tracker -l job-type=fopr-metadata-import

# Check CronJob schedules
kubectl get cronjobs -n rain-tracker

# View recent CronJob executions
kubectl get jobs -n rain-tracker -l job-type=historical-import-cron
```

### View Logs

```bash
# Live tail of job logs (follow)
kubectl logs -f -n rain-tracker -l job-type=historical-single-year

# View logs of specific job
kubectl logs -n rain-tracker job/historical-wy-import-abc123

# View logs of all jobs (paginated)
kubectl logs -n rain-tracker -l app=rain-tracker --tail=100

# Export logs to file
kubectl logs -n rain-tracker job/historical-wy-import-abc123 > import.log
```

### Common Issues

#### Issue 1: Job Fails Immediately
**Symptoms**: Job status shows "Error" or "Failed"

**Troubleshooting**:
```bash
# Check job events
kubectl describe job <job-name> -n rain-tracker

# Check pod events
kubectl get pods -n rain-tracker -l job-type=historical-single-year
kubectl describe pod <pod-name> -n rain-tracker

# Common causes:
# - Missing DATABASE_URL secret
# - Image pull errors
# - Resource constraints
```

**Solutions**:
```bash
# Verify secrets exist
kubectl get secret db-secrets -n rain-tracker

# Check resource quotas
kubectl describe resourcequota -n rain-tracker

# Verify image
kubectl get job <job-name> -n rain-tracker -o jsonpath='{.spec.template.spec.containers[0].image}'
```

#### Issue 2: Job Hangs or Times Out
**Symptoms**: Job runs for extended period without completing

**Troubleshooting**:
```bash
# Check pod status
kubectl get pods -n rain-tracker -l job-type=historical-bulk-import

# View live logs
kubectl logs -f -n rain-tracker <pod-name>

# Check resource usage
kubectl top pod <pod-name> -n rain-tracker

# Common causes:
# - Database connection issues
# - Network connectivity to MCFCD
# - Insufficient memory/CPU
```

**Solutions**:
```bash
# Test database connection
kubectl exec -it <pod-name> -n rain-tracker -- bash
psql "$DATABASE_URL" -c "SELECT 1;"

# Test MCFCD connectivity
curl -I https://alert.fcd.maricopa.gov/alert/Rain/

# Increase resources in job manifest
resources:
  limits:
    memory: "4Gi"  # Increase from 2Gi
    cpu: "2000m"
```

#### Issue 3: Import Completes But No Data
**Symptoms**: Job shows "Completed" but database has no new readings

**Troubleshooting**:
```bash
# Check import logs for errors
kubectl logs -n rain-tracker <pod-name> | grep -i "error\|failed\|warning"

# Verify data in database
kubectl exec -it <postgres-pod> -n rain-tracker -- psql -U rain_tracker
SELECT data_source, COUNT(*) FROM rain_readings GROUP BY data_source;

# Check if files were downloaded
kubectl logs -n rain-tracker <pod-name> | grep -i "download"

# Common causes:
# - 404 errors from MCFCD (file doesn't exist)
# - Parsing errors (malformed data)
# - Database constraint violations (duplicates)
```

**Solutions**:
```bash
# Verify file exists on MCFCD
curl -I https://alert.fcd.maricopa.gov/alert/Rain/pcp_WY_2023.xlsx

# Check for constraint violations in logs
kubectl logs -n rain-tracker <pod-name> | grep "constraint"

# Try importing with verbose logging
env:
  - name: RUST_LOG
    value: "debug"  # Instead of "info"
```

#### Issue 4: CronJob Not Running
**Symptoms**: CronJob exists but no jobs are created at scheduled time

**Troubleshooting**:
```bash
# Check CronJob status
kubectl get cronjob historical-import-current-year -n rain-tracker

# View CronJob details
kubectl describe cronjob historical-import-current-year -n rain-tracker

# Check recent executions
kubectl get jobs -n rain-tracker -l job-type=historical-import-cron

# Common causes:
# - Incorrect cron schedule syntax
# - CronJob suspended
# - Concurrency policy blocking
```

**Solutions**:
```bash
# Verify cron schedule (use https://crontab.guru/)
kubectl get cronjob <name> -n rain-tracker -o jsonpath='{.spec.schedule}'

# Check if suspended
kubectl get cronjob <name> -n rain-tracker -o jsonpath='{.spec.suspend}'

# Manually trigger CronJob (for testing)
kubectl create job --from=cronjob/historical-import-current-year test-run-1 -n rain-tracker

# Fix concurrency if needed
kubectl edit cronjob historical-import-current-year -n rain-tracker
# Set: concurrencyPolicy: Replace  # Instead of Forbid
```

---

## Performance Tuning

### Batch Size

Default: 1000 rows per transaction

**Increase for better performance**:
```yaml
env:
  - name: BATCH_SIZE
    value: "5000"  # Larger batches, fewer transactions
```

**Trade-offs**:
- Larger batches: Faster imports, higher memory usage
- Smaller batches: Slower imports, lower memory usage, better for constrained environments

### Resource Allocation

**For single year imports** (small):
```yaml
resources:
  requests:
    memory: "256Mi"
    cpu: "250m"
  limits:
    memory: "1Gi"
    cpu: "1000m"
```

**For bulk imports** (large):
```yaml
resources:
  requests:
    memory: "1Gi"
    cpu: "1000m"
  limits:
    memory: "4Gi"
    cpu: "2000m"
```

### Parallel Processing

**Current**: Sequential processing (one gauge at a time)

**Future**: Parallel gauge processing (not yet implemented)

```yaml
env:
  - name: PARALLEL_GAUGE_PROCESSING
    value: "true"
  - name: MAX_PARALLEL_GAUGES
    value: "10"
```

See `docs/implementation-task-breakdown.md` (R9) for parallel processing roadmap.

---

## Data Validation

### Verify Import Success

```sql
-- Count readings by data source
SELECT data_source, COUNT(*) as count,
       MIN(reading_date) as first_date,
       MAX(reading_date) as last_date
FROM rain_readings
GROUP BY data_source
ORDER BY data_source;

-- Check coverage for specific water year
SELECT COUNT(DISTINCT station_id) as gauge_count,
       COUNT(*) as reading_count,
       MIN(reading_date) as first_date,
       MAX(reading_date) as last_date
FROM rain_readings
WHERE reading_date >= '2022-10-01'
  AND reading_date < '2023-10-01'
  AND data_source LIKE 'excel_WY_%';

-- Verify monthly summaries were recalculated
SELECT water_year, COUNT(*) as months,
       SUM(total_precipitation) as total_rain
FROM monthly_rainfall_summary
WHERE water_year = 2023
GROUP BY water_year;

-- Check for data quality issues
SELECT station_id, reading_date, rainfall_inches
FROM rain_readings
WHERE rainfall_inches > 20.0  -- Suspicious high values
ORDER BY rainfall_inches DESC
LIMIT 100;
```

### Compare Import Methods

For year 2022, both Excel and PDF formats exist. Validate consistency:

```sql
-- Compare Excel vs PDF imports for 2022
SELECT data_source,
       COUNT(*) as reading_count,
       SUM(rainfall_inches) as total_rainfall,
       AVG(rainfall_inches) as avg_rainfall
FROM rain_readings
WHERE reading_date >= '2021-10-01'
  AND reading_date < '2022-10-01'
GROUP BY data_source;
```

---

## Cleanup

### Delete Completed Jobs

```bash
# Delete all completed jobs older than 24 hours (automatic with ttlSecondsAfterFinished)
# Manual cleanup:
kubectl delete jobs -n rain-tracker -l app=rain-tracker --field-selector status.successful=1

# Delete all failed jobs
kubectl delete jobs -n rain-tracker -l app=rain-tracker --field-selector status.failed=1

# Delete specific job
kubectl delete job historical-wy-import-abc123 -n rain-tracker
```

### Suspend CronJobs

```bash
# Temporarily disable daily imports
kubectl patch cronjob historical-import-current-year -n rain-tracker -p '{"spec":{"suspend":true}}'

# Re-enable
kubectl patch cronjob historical-import-current-year -n rain-tracker -p '{"spec":{"suspend":false}}'

# Delete CronJob entirely
kubectl delete cronjob historical-import-current-year -n rain-tracker
```

### Remove All Import Jobs

```bash
# WARNING: This deletes all import jobs and CronJobs
kubectl delete jobs -n rain-tracker -l app=rain-tracker,component=import-jobs
kubectl delete cronjobs -n rain-tracker -l app=rain-tracker,component=import-jobs
kubectl delete configmap historical-import-config -n rain-tracker
```

---

## Best Practices

### 1. Test Locally First

Before running bulk imports in K8s:

```bash
# Test with local binary
export DATABASE_URL="postgres://localhost/rain_tracker_test"
./target/debug/historical-import single --water-year 2023 --yes

# Verify data
psql $DATABASE_URL -c "SELECT COUNT(*) FROM rain_readings WHERE data_source = 'excel_WY_2023';"
```

### 2. Start Small, Scale Up

```bash
# 1. Import one recent year
./scripts/import-water-year.sh 2024

# 2. Verify success
kubectl logs -f -n rain-tracker -l job-type=historical-single-year

# 3. Import small range
# Edit bulk manifest: START_YEAR=2022, END_YEAR=2024
kubectl create -f k8s/jobs/historical-bulk-import.yaml

# 4. Import all historical data
# Edit bulk manifest: START_YEAR=2010, END_YEAR=2024
kubectl create -f k8s/jobs/historical-bulk-import.yaml
```

### 3. Monitor Resource Usage

```bash
# Watch pod resource consumption
kubectl top pods -n rain-tracker -l app=rain-tracker

# Set alerts for failures
# (Requires Prometheus/Alertmanager setup)
```

### 4. Use ConfigMaps for Environment Parity

```bash
# Development
kubectl apply -f k8s/jobs/import-job-config.yaml -n rain-tracker-dev

# Production
kubectl apply -f k8s/jobs/import-job-config.yaml -n rain-tracker-prod

# Override RUST_LOG for production
kubectl edit configmap historical-import-config -n rain-tracker-prod
# Change: RUST_LOG: "info"  (instead of "debug")
```

### 5. Archive Downloaded Files (Optional)

If you want to keep downloaded Excel/PDF files:

```yaml
# Add volume mount to job spec
volumeMounts:
  - name: data-archive
    mountPath: /archive

volumes:
  - name: data-archive
    persistentVolumeClaim:
      claimName: historical-data-pvc
```

Then modify import script to copy files to `/archive` before import.

---

## Security Considerations

### 1. Secrets Management

**Don't do this** ❌:
```yaml
env:
  - name: DATABASE_URL
    value: "postgres://user:password@host/db"  # Hardcoded password!
```

**Do this** ✅:
```yaml
env:
  - name: DATABASE_URL
    valueFrom:
      secretKeyRef:
        name: db-secrets
        key: DATABASE_URL
```

**Even better** (production):
- Use SealedSecrets (Bitnami)
- Use Vault (HashiCorp)
- Use cloud provider secret managers (AWS Secrets Manager, GCP Secret Manager)

### 2. RBAC

Jobs should use minimal permissions:

```bash
# Apply service account with limited permissions
kubectl apply -f k8s/jobs/import-job-config.yaml

# Add to job spec
spec:
  template:
    spec:
      serviceAccountName: historical-import-sa
```

### 3. Network Policies

Restrict job network access:

```yaml
apiVersion: networking.k8s.io/v1
kind: NetworkPolicy
metadata:
  name: import-job-netpol
spec:
  podSelector:
    matchLabels:
      component: import-jobs
  policyTypes:
    - Egress
  egress:
    # Allow database access
    - to:
        - podSelector:
            matchLabels:
              app: postgres
      ports:
        - protocol: TCP
          port: 5432
    # Allow MCFCD downloads
    - to:
        - namespaceSelector: {}
      ports:
        - protocol: TCP
          port: 443
```

---

## Reference

### Job Manifest Files

| File | Purpose | When to Use |
|------|---------|-------------|
| `historical-single-year-import.yaml` | Import one water year | One-off imports, backfilling specific years |
| `historical-bulk-import.yaml` | Import range of years | Initial data load, large backfills |
| `fopr-metadata-import.yaml` | Import gauge metadata | Metadata population, availability checks |
| `historical-import-cronjob.yaml` | Scheduled imports | Ongoing automated updates |
| `import-job-config.yaml` | Shared configuration | Centralized settings, RBAC |

### Helper Scripts

| Script | Purpose | Usage |
|--------|---------|-------|
| `scripts/import-water-year.sh` | Import single year via K8s | `./scripts/import-water-year.sh 2023` |
| `scripts/verify-fopr-migration.sh` | Verify database schema | `./scripts/verify-fopr-migration.sh` |

### Related Documentation

- [Implementation Task Breakdown](../../docs/implementation-task-breakdown.md) - Detailed feature breakdown, roadmap
- [FOPR Parsing Specification](../../docs/fopr-meta-stats-parsing-spec.md) - FOPR metadata extraction details
- [CLAUDE.md](../../CLAUDE.md) - Development guidelines, architecture overview
- [README.md](../../README.md) - Main project documentation

### External Resources

- [Kubernetes Jobs Documentation](https://kubernetes.io/docs/concepts/workloads/controllers/job/)
- [Kubernetes CronJobs Documentation](https://kubernetes.io/docs/concepts/workloads/controllers/cron-jobs/)
- [Cron Schedule Helper](https://crontab.guru/) - Validate cron expressions
- [MCFCD Website](https://alert.fcd.maricopa.gov/) - Data source

---

## Support

### Troubleshooting Checklist

Before opening an issue, verify:

- [ ] Database is accessible (check `DATABASE_URL` secret)
- [ ] Migrations are applied (`kubectl exec` into postgres, run `\dt`)
- [ ] MCFCD website is accessible (`curl -I https://alert.fcd.maricopa.gov/`)
- [ ] Job logs show actual error (`kubectl logs`)
- [ ] Resource limits are sufficient (`kubectl top pod`)
- [ ] File exists on MCFCD (check specific URL in logs)

### Getting Help

1. **Check logs**: `kubectl logs -n rain-tracker <pod-name>`
2. **Check events**: `kubectl describe job <job-name> -n rain-tracker`
3. **Test locally**: Run `historical-import` binary locally with same parameters
4. **Review docs**: `docs/implementation-task-breakdown.md`, `CLAUDE.md`
5. **Open issue**: Provide logs, manifest, error message

---

**Last Updated**: 2025-10-26
**Version**: 1.0
**Maintainer**: Rain Tracker Team

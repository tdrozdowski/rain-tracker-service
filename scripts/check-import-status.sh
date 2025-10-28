#!/bin/bash
# Check status of all historical import jobs and CronJobs
# Usage: ./scripts/check-import-status.sh

set -e

echo "=================================================="
echo "Historical Import Job Status"
echo "=================================================="
echo ""

echo "📊 Active Jobs:"
echo ""
kubectl get jobs -n rain-tracker -l app=rain-tracker 2>/dev/null || echo "No active jobs found"

echo ""
echo "=================================================="
echo ""

echo "📅 CronJobs:"
echo ""
kubectl get cronjobs -n rain-tracker -l app=rain-tracker 2>/dev/null || echo "No CronJobs configured"

echo ""
echo "=================================================="
echo ""

echo "🏃 Running Pods:"
echo ""
kubectl get pods -n rain-tracker -l app=rain-tracker,component=import-jobs 2>/dev/null || echo "No import pods running"

echo ""
echo "=================================================="
echo ""

echo "📈 Recent Job History (last 10):"
echo ""
kubectl get jobs -n rain-tracker -l app=rain-tracker --sort-by=.metadata.creationTimestamp 2>/dev/null | tail -n 11 || echo "No job history"

echo ""
echo "=================================================="
echo ""

echo "💾 Database Import Statistics:"
echo ""

# Try to get database statistics
DB_POD=$(kubectl get pods -n rain-tracker -l app=postgres -o jsonpath='{.items[0].metadata.name}' 2>/dev/null)

if [ -n "$DB_POD" ]; then
  echo "Readings by data source:"
  kubectl exec -it "$DB_POD" -n rain-tracker -- psql -U rain_tracker -c "
    SELECT data_source,
           COUNT(*) as reading_count,
           MIN(reading_date) as first_date,
           MAX(reading_date) as last_date
    FROM rain_readings
    GROUP BY data_source
    ORDER BY data_source;
  " 2>/dev/null || echo "Could not query database"

  echo ""
  echo "Water year coverage:"
  kubectl exec -it "$DB_POD" -n rain-tracker -- psql -U rain_tracker -c "
    SELECT EXTRACT(YEAR FROM reading_date + INTERVAL '3 months') as water_year,
           COUNT(*) as reading_count,
           COUNT(DISTINCT station_id) as gauge_count
    FROM rain_readings
    GROUP BY water_year
    ORDER BY water_year DESC
    LIMIT 15;
  " 2>/dev/null || echo "Could not query database"
else
  echo "⚠️  Cannot connect to database pod"
fi

echo ""
echo "=================================================="
echo ""

echo "💡 Useful commands:"
echo ""
echo "View logs of latest job:"
echo "  kubectl logs -n rain-tracker -l app=rain-tracker --tail=100"
echo ""
echo "Follow logs in real-time:"
echo "  kubectl logs -f -n rain-tracker -l job-type=historical-single-year"
echo ""
echo "Delete completed jobs:"
echo "  kubectl delete jobs -n rain-tracker --field-selector status.successful=1"
echo ""

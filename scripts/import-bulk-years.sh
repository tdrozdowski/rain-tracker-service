#!/bin/bash
# Import a range of water years using K8s bulk import job
# Usage: ./scripts/import-bulk-years.sh 2010 2024

set -e

START_YEAR=${1:-2010}
END_YEAR=${2:-2024}

if [ -z "$1" ] || [ -z "$2" ]; then
  echo "Usage: $0 <start_year> <end_year>"
  echo "Example: $0 2010 2024"
  exit 1
fi

echo "🚀 Starting bulk import for water years $START_YEAR to $END_YEAR..."
echo ""

cat k8s/jobs/base/historical-bulk-import.yaml | \
  sed "s/value: \"2010\"/value: \"$START_YEAR\"/" | \
  sed "s/value: \"2024\"/value: \"$END_YEAR\"/" | \
  kubectl create -f -

echo ""
echo "✅ Bulk import job created for water years $START_YEAR-$END_YEAR"
echo ""
echo "Monitor with:"
echo "   kubectl logs -f -n rain-tracker -l job-type=historical-bulk-import-sequential"
echo ""
echo "Check status with:"
echo "   kubectl get jobs -n rain-tracker -l job-type=historical-bulk-import-sequential"
echo ""
echo "Estimated duration: $((2 * (END_YEAR - START_YEAR + 1))) - $((3 * (END_YEAR - START_YEAR + 1))) minutes"
echo ""

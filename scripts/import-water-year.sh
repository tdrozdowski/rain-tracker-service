#!/bin/bash
# Import a single water year of historical rain data
# Usage: ./scripts/import-water-year.sh 2023

set -e

WATER_YEAR=${1:-2023}

echo "🚀 Starting import for water year $WATER_YEAR..."

cat k8s/jobs/base/historical-single-year-import.yaml | \
  sed "s/value: \"2023\"/value: \"$WATER_YEAR\"/" | \
  kubectl create -f -

echo ""
echo "✅ Job created for water year $WATER_YEAR"
echo ""
echo "Monitor with:"
echo "   kubectl logs -f -l job-type=historical-single-year"
echo ""
echo "Check status with:"
echo "   kubectl get jobs -l job-type=historical-single-year"

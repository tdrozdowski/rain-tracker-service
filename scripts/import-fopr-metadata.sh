#!/bin/bash
# Import FOPR metadata for all gauges or a specific gauge
# Usage:
#   ./scripts/import-fopr-metadata.sh           # All gauges
#   ./scripts/import-fopr-metadata.sh 59700     # Specific gauge

set -e

if [ -z "$1" ]; then
  # Import all gauges
  echo "🚀 Starting FOPR metadata import for all gauges..."
  echo ""

  kubectl create -f k8s/jobs/base/fopr-metadata-import.yaml

  echo ""
  echo "✅ FOPR metadata import job created for all active gauges"
  echo ""
  echo "Monitor with:"
  echo "   kubectl logs -f -n rain-tracker -l job-type=fopr-metadata-import"
  echo ""
  echo "Check status with:"
  echo "   kubectl get jobs -n rain-tracker -l job-type=fopr-metadata-import"
  echo ""
  echo "Estimated duration: ~12-15 minutes for 350 gauges"
  echo ""
else
  # Import specific gauge
  STATION_ID=$1

  echo "🚀 Starting FOPR metadata import for gauge $STATION_ID..."
  echo ""

  # Extract the single-gauge job definition (second yaml document)
  cat k8s/jobs/base/fopr-metadata-import.yaml | \
    awk '/^---$/,0' | \
    tail -n +2 | \
    sed "s/value: \"59700\"/value: \"$STATION_ID\"/" | \
    kubectl create -f -

  echo ""
  echo "✅ FOPR metadata import job created for gauge $STATION_ID"
  echo ""
  echo "Monitor with:"
  echo "   kubectl logs -f -n rain-tracker -l job-type=fopr-single-gauge"
  echo ""
  echo "Check status with:"
  echo "   kubectl get jobs -n rain-tracker -l job-type=fopr-single-gauge"
  echo ""
fi

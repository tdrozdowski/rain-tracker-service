#!/bin/bash
# Set up automated CronJobs for historical data imports
# Usage: ./scripts/setup-import-cronjobs.sh

set -e

echo "🚀 Setting up historical import CronJobs..."
echo ""

# Apply CronJob manifests
# Note: You can also use Kustomize overlays for environment-specific configs
# kubectl apply -k k8s/jobs/overlays/production
kubectl apply -f k8s/jobs/base/historical-import-cronjob.yaml

echo ""
echo "✅ CronJobs created successfully!"
echo ""

# Display created CronJobs
echo "📅 Scheduled CronJobs:"
echo ""
kubectl get cronjobs -n rain-tracker -l job-type -o custom-columns=\
NAME:.metadata.name,\
SCHEDULE:.spec.schedule,\
SUSPEND:.spec.suspend,\
ACTIVE:.status.active,\
LAST_SCHEDULE:.status.lastScheduleTime

echo ""
echo "Next scheduled runs:"
echo ""

# Show when each CronJob will run next
for cronjob in $(kubectl get cronjobs -n rain-tracker -l job-type -o jsonpath='{.items[*].metadata.name}'); do
  echo "  $cronjob:"
  kubectl get cronjob "$cronjob" -n rain-tracker -o jsonpath='    Schedule: {.spec.schedule}{"\n"}'
  # Note: Next run time requires manual calculation or external tool
done

echo ""
echo "💡 To manually trigger a CronJob for testing:"
echo "   kubectl create job --from=cronjob/<cronjob-name> test-run-1 -n rain-tracker"
echo ""
echo "💡 To suspend a CronJob temporarily:"
echo "   kubectl patch cronjob <cronjob-name> -n rain-tracker -p '{\"spec\":{\"suspend\":true}}'"
echo ""
echo "💡 To view CronJob execution history:"
echo "   kubectl get jobs -n rain-tracker -l job-type=historical-import-cron"
echo ""

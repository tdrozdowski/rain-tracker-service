# Kustomize Guide for Historical Import Jobs

This directory uses **Kustomize** to manage Kubernetes manifests for historical data import jobs across multiple environments.

## Directory Structure

```
k8s/jobs/
├── base/                           # Base configuration (shared)
│   ├── kustomization.yaml          # Base Kustomize config
│   ├── import-job-config.yaml      # ConfigMap, Secret, RBAC
│   ├── historical-import-cronjob.yaml  # CronJob definitions
│   ├── historical-single-year-import.yaml  # Job template
│   ├── historical-bulk-import.yaml        # Job template
│   └── fopr-metadata-import.yaml          # Job template
├── overlays/
│   ├── dev/                        # Development environment
│   │   ├── kustomization.yaml
│   │   └── cronjob-suspend.yaml    # Suspend CronJobs in dev
│   ├── staging/                    # Staging environment
│   │   └── kustomization.yaml
│   └── production/                 # Production environment
│       └── kustomization.yaml
├── README.md                       # Detailed usage guide
└── KUSTOMIZE.md                    # This file
```

---

## Quick Start

### Deploy to Development

```bash
# Apply ConfigMap and RBAC only
kubectl apply -k k8s/jobs/overlays/dev

# Verify
kubectl get configmap historical-import-config -n rain-tracker-dev
kubectl get serviceaccount historical-import-sa -n rain-tracker-dev
```

### Deploy to Production

```bash
# Review what will be deployed
kubectl kustomize k8s/jobs/overlays/production

# Apply to cluster
kubectl apply -k k8s/jobs/overlays/production

# Verify CronJobs are created and scheduled
kubectl get cronjobs -n rain-tracker
```

---

## Understanding the Structure

### Base Layer (`base/`)

The base layer contains:
- ✅ **ConfigMap**: Shared configuration for all import jobs
- ✅ **Secret template**: Database credentials structure
- ✅ **RBAC**: ServiceAccount, Role, RoleBinding
- ⚠️ **CronJobs**: Commented out by default (enable in overlays)
- ❌ **Job templates**: NOT included (use scripts instead)

**Why aren't Job templates included?**

Jobs with `generateName` don't work with `kubectl apply`:
```yaml
metadata:
  generateName: historical-wy-import-  # ❌ Requires kubectl create
```

Instead, use the helper scripts which handle job creation properly:
- `./scripts/import-water-year.sh`
- `./scripts/import-bulk-years.sh`
- `./scripts/import-fopr-metadata.sh`

### Overlay Layers (`overlays/*/`)

Each environment overlay can customize:
- **Namespace**: Different namespaces per environment
- **Labels**: Environment-specific labels
- **ConfigMap values**: Logging levels, batch sizes, etc.
- **Image tags**: `dev-latest`, `v0.3.0-rc.1`, `v0.3.0`
- **Resource limits**: Higher limits in production
- **CronJob schedules**: Different frequencies per environment
- **CronJob suspend state**: Auto-suspend in dev

---

## Common Workflows

### 1. Deploy Base Configuration Only

Deploy just the ConfigMap and RBAC (no CronJobs):

```bash
# Development
kubectl apply -k k8s/jobs/overlays/dev

# Production
kubectl apply -k k8s/jobs/overlays/production
```

### 2. Deploy with CronJobs Enabled

Edit `base/kustomization.yaml` and uncomment CronJobs:

```yaml
resources:
  - import-job-config.yaml
  - historical-import-cronjob.yaml  # ← Uncomment this
```

Then apply:
```bash
kubectl apply -k k8s/jobs/overlays/production
```

**OR** add CronJobs to overlay-specific resources:

```yaml
# In overlays/production/kustomization.yaml
resources:
  - ../../base
  - ../../base/historical-import-cronjob.yaml  # Add CronJobs only in prod
```

### 3. Create On-Demand Jobs (Recommended)

Use helper scripts that work with any environment:

```bash
# Development namespace
export NAMESPACE=rain-tracker-dev
./scripts/import-water-year.sh 2023

# Production namespace
export NAMESPACE=rain-tracker
./scripts/import-water-year.sh 2023
```

Or use Kustomize to generate job manifest, then create it:

```bash
# Generate job manifest with dev overlay
kubectl kustomize k8s/jobs/overlays/dev > /tmp/job.yaml

# Edit the job template (add it to kustomization first)
# ... or just use the script

# Create the job
kubectl create -f /tmp/job.yaml -n rain-tracker-dev
```

### 4. Update Configuration Across Environments

**Scenario**: Change batch size from 1000 to 2000

**Option A**: Update base (affects all environments)
```yaml
# base/import-job-config.yaml
data:
  BATCH_SIZE: "2000"  # Change from 1000
```

**Option B**: Update specific environment overlay
```yaml
# overlays/production/kustomization.yaml
configMapGenerator:
  - name: historical-import-config
    behavior: merge
    literals:
      - BATCH_SIZE=2000  # Override for production only
```

Apply changes:
```bash
kubectl apply -k k8s/jobs/overlays/production
```

---

## Environment Configurations

### Development (`overlays/dev`)

**Characteristics**:
- Namespace: `rain-tracker-dev`
- Image: `dev-latest` (automatic builds)
- Logging: `RUST_LOG=debug` (verbose)
- CronJobs: **Suspended** by default
- Error handling: Continue on error

**When to use**: Local development, testing import jobs

**Deploy**:
```bash
kubectl apply -k k8s/jobs/overlays/dev
```

**Enable CronJobs** (for testing):
```bash
kubectl patch cronjob historical-import-current-year -n rain-tracker-dev \
  -p '{"spec":{"suspend":false}}'
```

### Staging (`overlays/staging`)

**Characteristics**:
- Namespace: `rain-tracker-staging`
- Image: `v0.3.0-rc.1` (release candidates)
- Logging: `RUST_LOG=info`
- CronJobs: Enabled but **less frequent** schedules
  - Daily: 4 AM (instead of 3 AM)
  - Weekly: Saturdays (instead of Sundays)
- Error handling: Fail fast

**When to use**: Pre-production validation, QA testing

**Deploy**:
```bash
kubectl apply -k k8s/jobs/overlays/staging
```

### Production (`overlays/production`)

**Characteristics**:
- Namespace: `rain-tracker`
- Image: `v0.3.0` (specific version tags, never `latest`)
- Logging: `RUST_LOG=info`
- CronJobs: Enabled with **standard schedules**
- Error handling: Fail fast
- Resources: **Higher limits** (2Gi RAM, 2 CPU)
- Batch size: **2000** (vs 1000 in dev)

**When to use**: Production deployments

**Deploy**:
```bash
# Always review first!
kubectl kustomize k8s/jobs/overlays/production

# Apply to production
kubectl apply -k k8s/jobs/overlays/production
```

---

## Advanced Customization

### Adding a New Environment

Create a new overlay:

```bash
mkdir -p k8s/jobs/overlays/qa
```

Create `k8s/jobs/overlays/qa/kustomization.yaml`:

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: rain-tracker-qa

resources:
  - ../../base

commonLabels:
  environment: qa

configMapGenerator:
  - name: historical-import-config
    behavior: merge
    literals:
      - RUST_LOG=info
      - BATCH_SIZE=1500

images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: qa-latest
```

Deploy:
```bash
kubectl apply -k k8s/jobs/overlays/qa
```

### Using ConfigMapGenerator

Override specific config values per environment:

```yaml
# overlays/production/kustomization.yaml
configMapGenerator:
  - name: historical-import-config
    behavior: merge  # Merge with base ConfigMap
    literals:
      - RUST_LOG=warn  # More restrictive in production
      - BATCH_SIZE=5000  # Larger batches
      - VALIDATE_RAINFALL_MAX=25.0
```

### Image Tag Management

**Development**: Use `latest` or `dev-latest`
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: dev-latest
```

**Staging**: Use release candidates
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: v0.3.0-rc.1
```

**Production**: Use specific versions
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: v0.3.0
```

### Resource Limit Patches

Increase resources for specific CronJobs:

```yaml
# overlays/production/kustomization.yaml
patches:
  - patch: |-
      - op: replace
        path: /spec/jobTemplate/spec/template/spec/containers/0/resources/limits/memory
        value: "4Gi"
      - op: replace
        path: /spec/jobTemplate/spec/template/spec/containers/0/resources/limits/cpu
        value: "4000m"
    target:
      kind: CronJob
      name: historical-import-current-year
```

### Schedule Patches

Change CronJob schedules per environment:

```yaml
# overlays/staging/kustomization.yaml
patches:
  - patch: |-
      - op: replace
        path: /spec/schedule
        value: "0 5 * * *"  # 5 AM in staging
    target:
      kind: CronJob
      name: historical-import-current-year
```

---

## Validation

### Validate Kustomization

Check that your Kustomize configuration is valid:

```bash
# Validate base
kubectl kustomize k8s/jobs/base

# Validate specific overlay
kubectl kustomize k8s/jobs/overlays/production

# Validate with kubeval (if installed)
kubectl kustomize k8s/jobs/overlays/production | kubeval
```

### Dry Run

Preview what will be applied without actually applying:

```bash
# Generate and preview
kubectl kustomize k8s/jobs/overlays/production

# Dry run apply
kubectl apply -k k8s/jobs/overlays/production --dry-run=client

# Server-side dry run (validates against API server)
kubectl apply -k k8s/jobs/overlays/production --dry-run=server
```

### Diff Changes

See what would change before applying:

```bash
# Requires kubectl diff
kubectl diff -k k8s/jobs/overlays/production
```

---

## Troubleshooting

### Issue: "resource from file is a CronJob but field is a Job"

**Cause**: Mixing Job and CronJob resources with same name

**Solution**: Ensure job templates and CronJobs have different names

### Issue: "generateName is not supported with apply"

**Cause**: Trying to `kubectl apply` a Job with `generateName`

**Solution**: Use `kubectl create` or use helper scripts:
```bash
kubectl create -f k8s/jobs/base/historical-single-year-import.yaml
# OR
./scripts/import-water-year.sh 2023
```

### Issue: ConfigMap not updating

**Cause**: ConfigMapGenerator creates new ConfigMaps with hashes

**Solution**: Use `behavior: merge` or delete and recreate:
```bash
kubectl delete configmap historical-import-config -n rain-tracker
kubectl apply -k k8s/jobs/overlays/production
```

### Issue: Image tag not changing

**Cause**: Kustomize image transformation not applied

**Solution**: Check images section in kustomization.yaml:
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service  # Must match exactly
    newTag: v0.3.0
```

---

## Integration with CI/CD

### GitHub Actions Example

```yaml
# .github/workflows/deploy-import-jobs.yml
name: Deploy Import Jobs

on:
  push:
    branches: [main]
    paths:
      - 'k8s/jobs/**'

jobs:
  deploy:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup kubectl
        uses: azure/setup-kubectl@v3

      - name: Deploy to production
        run: |
          kubectl kustomize k8s/jobs/overlays/production
          kubectl apply -k k8s/jobs/overlays/production
        env:
          KUBECONFIG: ${{ secrets.KUBECONFIG_PROD }}
```

### ArgoCD Example

```yaml
# argocd/import-jobs-app.yaml
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: rain-tracker-import-jobs
  namespace: argocd
spec:
  project: default
  source:
    repoURL: https://github.com/your-org/rain-tracker-service
    targetRevision: main
    path: k8s/jobs/overlays/production
  destination:
    server: https://kubernetes.default.svc
    namespace: rain-tracker
  syncPolicy:
    automated:
      prune: true
      selfHeal: true
```

---

## Best Practices

### 1. Never Use `latest` in Production
```yaml
# ❌ Bad
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: latest

# ✅ Good
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: v0.3.0
```

### 2. Always Review Before Applying to Production
```bash
# Review changes
kubectl diff -k k8s/jobs/overlays/production

# Dry run
kubectl apply -k k8s/jobs/overlays/production --dry-run=server

# Then apply
kubectl apply -k k8s/jobs/overlays/production
```

### 3. Use Environment Variables for Secrets
Never commit secrets to git. Use external secret management:

```yaml
# Use sealed-secrets, vault, or cloud provider
# Example with sealed-secrets:
apiVersion: bitnami.com/v1alpha1
kind: SealedSecret
metadata:
  name: db-secrets
spec:
  encryptedData:
    DATABASE_URL: AgBx8...  # Encrypted value
```

### 4. Keep Base Minimal
Only include resources shared across ALL environments in base.
Environment-specific resources go in overlays.

### 5. Test in Dev First
```bash
# Test in dev
kubectl apply -k k8s/jobs/overlays/dev

# Promote to staging
kubectl apply -k k8s/jobs/overlays/staging

# Finally production
kubectl apply -k k8s/jobs/overlays/production
```

---

## Reference

### Kustomize Documentation
- [Kustomize Official Docs](https://kustomize.io/)
- [Kubectl Kustomize](https://kubernetes.io/docs/tasks/manage-kubernetes-objects/kustomization/)

### Related Files
- `README.md` - Detailed job usage guide
- `base/import-job-config.yaml` - ConfigMap, RBAC definitions
- `../kustomization.yaml` - Main service Kustomize config

### Helper Scripts
All scripts work with Kustomize-deployed resources:
- `scripts/import-water-year.sh` - Single year import
- `scripts/import-bulk-years.sh` - Bulk import
- `scripts/import-fopr-metadata.sh` - FOPR metadata
- `scripts/setup-import-cronjobs.sh` - Deploy CronJobs
- `scripts/check-import-status.sh` - Check status

---

**Last Updated**: 2025-10-27
**Version**: 1.0
**Kustomize Version**: v1beta1

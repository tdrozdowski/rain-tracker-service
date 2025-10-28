# Kustomize Setup Summary for Historical Import Jobs

**Date**: 2025-10-27
**Branch**: `feature/historical-data-import`

## Overview

The K8s jobs for historical data import have been restructured to use **Kustomize** for managing configurations across multiple environments (dev, staging, production).

---

## Directory Structure

```
k8s/jobs/
├── base/                                      # Base configuration (shared)
│   ├── kustomization.yaml                     # Base Kustomize config
│   ├── import-job-config.yaml                 # ConfigMap, Secret, RBAC, PVC
│   ├── historical-import-cronjob.yaml         # 3 CronJob definitions
│   ├── historical-single-year-import.yaml     # Job template
│   ├── historical-bulk-import.yaml            # Job template (2 variants)
│   └── fopr-metadata-import.yaml              # Job template (2 variants)
├── overlays/
│   ├── dev/
│   │   ├── kustomization.yaml                 # Dev overrides
│   │   └── cronjob-suspend.yaml               # Suspend CronJobs in dev
│   ├── staging/
│   │   └── kustomization.yaml                 # Staging overrides
│   └── production/
│       └── kustomization.yaml                 # Production overrides
├── README.md                                  # Detailed job usage guide (850 lines)
└── KUSTOMIZE.md                               # Kustomize-specific guide (450 lines)
```

---

## What Changed

### Before (Old Structure)
```
k8s/jobs/
├── historical-single-year-import.yaml
├── historical-bulk-import.yaml
├── fopr-metadata-import.yaml
├── historical-import-cronjob.yaml
├── import-job-config.yaml
└── README.md
```

**Issues**:
- ❌ No environment separation
- ❌ Hardcoded values for all environments
- ❌ No easy way to customize per environment
- ❌ Manual file editing required for config changes

### After (New Kustomize Structure)
```
k8s/jobs/
├── base/              # Shared base configuration
├── overlays/
│   ├── dev/           # Development customization
│   ├── staging/       # Staging customization
│   └── production/    # Production customization
├── README.md
└── KUSTOMIZE.md
```

**Benefits**:
- ✅ Environment-specific configurations
- ✅ Easy customization via overlays
- ✅ Centralized base configuration
- ✅ Automated image tag management
- ✅ ConfigMap merging and patching
- ✅ Resource limit customization per environment
- ✅ CronJob schedule customization
- ✅ Follows Kubernetes best practices

---

## Kustomize Overlays

### Development (`overlays/dev`)

**Purpose**: Local development and testing

**Customizations**:
- Namespace: `rain-tracker-dev`
- Image: `dev-latest`
- Logging: `RUST_LOG=debug` (verbose)
- CronJobs: **Suspended** by default
- Error handling: Continue on error

**Deploy**:
```bash
kubectl apply -k k8s/jobs/overlays/dev
```

---

### Staging (`overlays/staging`)

**Purpose**: Pre-production testing and QA

**Customizations**:
- Namespace: `rain-tracker-staging`
- Image: `v0.3.0-rc.1` (release candidates)
- Logging: `RUST_LOG=info`
- CronJobs: Enabled with **adjusted schedules**
  - Daily: 4 AM (vs 3 AM in prod)
  - Weekly: Saturdays (vs Sundays in prod)

**Deploy**:
```bash
kubectl apply -k k8s/jobs/overlays/staging
```

---

### Production (`overlays/production`)

**Purpose**: Production deployments

**Customizations**:
- Namespace: `rain-tracker`
- Image: `v0.3.0` (specific version, never `latest`)
- Logging: `RUST_LOG=info`
- CronJobs: Enabled with standard schedules
- Resources: **Higher limits**
  - Memory: 2Gi (vs 1Gi in base)
  - CPU: 2000m (vs 1000m in base)
- Batch size: 2000 (vs 1000 in base)

**Deploy**:
```bash
kubectl apply -k k8s/jobs/overlays/production
```

---

## Migration Guide

### From Old Structure to New

If you were using the old structure, here's how to migrate:

#### Old Way (Direct Manifests)
```bash
# Apply config
kubectl apply -f k8s/jobs/import-job-config.yaml

# Create job
kubectl create -f k8s/jobs/historical-single-year-import.yaml
```

#### New Way (Kustomize)
```bash
# Apply config with Kustomize
kubectl apply -k k8s/jobs/overlays/production

# Create job (same as before - scripts unchanged)
./scripts/import-water-year.sh 2023
```

### Updated Scripts

All helper scripts now reference the correct paths:

- `scripts/import-water-year.sh` → `k8s/jobs/base/historical-single-year-import.yaml`
- `scripts/import-bulk-years.sh` → `k8s/jobs/base/historical-bulk-import.yaml`
- `scripts/import-fopr-metadata.sh` → `k8s/jobs/base/fopr-metadata-import.yaml`
- `scripts/setup-import-cronjobs.sh` → `k8s/jobs/base/historical-import-cronjob.yaml`

**No changes to script usage** - they work the same way!

---

## Common Workflows

### 1. Deploy Configuration to Production

```bash
# Review what will be deployed
kubectl kustomize k8s/jobs/overlays/production

# Apply to cluster
kubectl apply -k k8s/jobs/overlays/production

# Verify
kubectl get configmap historical-import-config -n rain-tracker
kubectl get cronjobs -n rain-tracker
```

### 2. Deploy to Development

```bash
# Apply dev configuration (CronJobs suspended)
kubectl apply -k k8s/jobs/overlays/dev

# Test a single job
./scripts/import-water-year.sh 2024

# Enable CronJobs if needed
kubectl patch cronjob historical-import-current-year \
  -n rain-tracker-dev \
  -p '{"spec":{"suspend":false}}'
```

### 3. Customize for Your Environment

Create a new overlay:

```bash
mkdir -p k8s/jobs/overlays/my-env
```

Create `k8s/jobs/overlays/my-env/kustomization.yaml`:

```yaml
apiVersion: kustomize.config.k8s.io/v1beta1
kind: Kustomization

namespace: rain-tracker-my-env

resources:
  - ../../base

commonLabels:
  environment: my-env

images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: my-custom-tag
```

Deploy:
```bash
kubectl apply -k k8s/jobs/overlays/my-env
```

---

## Configuration Management

### ConfigMap Customization

**Base ConfigMap** (`base/import-job-config.yaml`):
```yaml
data:
  RUST_LOG: "info"
  BATCH_SIZE: "1000"
  MCFCD_BASE_URL: "https://alert.fcd.maricopa.gov/alert/Rain/"
```

**Override in Overlay** (`overlays/production/kustomization.yaml`):
```yaml
configMapGenerator:
  - name: historical-import-config
    behavior: merge  # Merge with base
    literals:
      - BATCH_SIZE=2000  # Override
      - RUST_LOG=warn     # Override
      # Other values from base remain unchanged
```

### Image Tag Management

**Base**:
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: latest
```

**Production Override**:
```yaml
images:
  - name: ghcr.io/your-org/rain-tracker-service
    newTag: v0.3.0  # Specific version
```

### Resource Limits

**Production Patch** (`overlays/production/kustomization.yaml`):
```yaml
patches:
  - patch: |-
      - op: replace
        path: /spec/jobTemplate/spec/template/spec/containers/0/resources/limits/memory
        value: "2Gi"
      - op: replace
        path: /spec/jobTemplate/spec/template/spec/containers/0/resources/limits/cpu
        value: "2000m"
    target:
      kind: CronJob
      name: historical-import-current-year
```

---

## Key Features

### 1. Environment Isolation
- Dev, staging, and production have separate configurations
- Different namespaces prevent accidental cross-environment actions
- Environment-specific labels for tracking

### 2. Configuration Inheritance
- Base configuration shared across all environments
- Overlays only specify differences
- DRY principle - don't repeat yourself

### 3. Image Tag Control
- Development: `dev-latest` (automatic)
- Staging: `v0.3.0-rc.1` (release candidates)
- Production: `v0.3.0` (specific versions)
- Never use `latest` in production!

### 4. Resource Optimization
- Dev: Lower resource limits (cost savings)
- Production: Higher limits (performance)
- Easy to adjust per environment

### 5. Schedule Flexibility
- Production: Standard schedules (daily, weekly, monthly)
- Staging: Offset schedules (avoid conflicts)
- Dev: Suspended by default (on-demand only)

---

## Files Added

### Kustomize Configuration Files
- `k8s/jobs/base/kustomization.yaml` (39 lines)
- `k8s/jobs/overlays/dev/kustomization.yaml` (40 lines)
- `k8s/jobs/overlays/dev/cronjob-suspend.yaml` (15 lines)
- `k8s/jobs/overlays/staging/kustomization.yaml` (45 lines)
- `k8s/jobs/overlays/production/kustomization.yaml` (50 lines)

### Documentation
- `k8s/jobs/KUSTOMIZE.md` (450 lines) - Kustomize usage guide
- `docs/kustomize-setup-summary.md` (this file)

### Files Moved
- All job YAMLs moved from `k8s/jobs/` to `k8s/jobs/base/`

### Files Updated
- `k8s/jobs/README.md` - Added Kustomize section
- `scripts/import-water-year.sh` - Updated path references
- `scripts/import-bulk-years.sh` - Updated path references
- `scripts/import-fopr-metadata.sh` - Updated path references
- `scripts/setup-import-cronjobs.sh` - Updated path references

---

## Validation

### Test the Kustomize Setup

```bash
# Validate base configuration
kubectl kustomize k8s/jobs/base

# Validate dev overlay
kubectl kustomize k8s/jobs/overlays/dev

# Validate staging overlay
kubectl kustomize k8s/jobs/overlays/staging

# Validate production overlay
kubectl kustomize k8s/jobs/overlays/production

# Dry run production deployment
kubectl apply -k k8s/jobs/overlays/production --dry-run=server
```

### Test Scripts Still Work

```bash
# Test single year import (should use k8s/jobs/base/...)
./scripts/import-water-year.sh 2024

# Test bulk import
./scripts/import-bulk-years.sh 2023 2024

# Test FOPR import
./scripts/import-fopr-metadata.sh 59700

# Test CronJob setup
./scripts/setup-import-cronjobs.sh
```

---

## Best Practices

### 1. Never Use `latest` in Production
```yaml
# ❌ Bad
newTag: latest

# ✅ Good
newTag: v0.3.0
```

### 2. Always Review Before Deploying
```bash
# Review changes
kubectl diff -k k8s/jobs/overlays/production

# Then apply
kubectl apply -k k8s/jobs/overlays/production
```

### 3. Test in Lower Environments First
```bash
# Dev → Staging → Production
kubectl apply -k k8s/jobs/overlays/dev
# Test...
kubectl apply -k k8s/jobs/overlays/staging
# Test...
kubectl apply -k k8s/jobs/overlays/production
```

### 4. Use Specific Versions
Tag your images with semantic versions:
- Dev: `dev-latest` or `dev-YYYYMMDD`
- Staging: `v0.3.0-rc.1`, `v0.3.0-rc.2`
- Production: `v0.3.0`, `v0.3.1`

### 5. Keep Base Minimal
Only put truly shared configuration in base.
Environment-specific settings go in overlays.

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
  deploy-prod:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v3

      - name: Setup kubectl
        uses: azure/setup-kubectl@v3

      - name: Deploy to production
        run: |
          kubectl apply -k k8s/jobs/overlays/production
        env:
          KUBECONFIG: ${{ secrets.KUBECONFIG_PROD }}
```

### ArgoCD Example

```yaml
# Import jobs application
apiVersion: argoproj.io/v1alpha1
kind: Application
metadata:
  name: rain-tracker-import-jobs
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

## Troubleshooting

### Issue: "no matches for kind"

**Cause**: Kustomize version too old

**Solution**: Update kubectl to 1.21+ or use standalone kustomize v4+

### Issue: ConfigMap not updating

**Cause**: ConfigMapGenerator creates new ConfigMaps with hashes

**Solution**: Delete and recreate:
```bash
kubectl delete configmap historical-import-config -n rain-tracker
kubectl apply -k k8s/jobs/overlays/production
```

Or use `behavior: merge` in configMapGenerator.

### Issue: Scripts can't find job templates

**Cause**: Files moved to base/ directory

**Solution**: Scripts updated to use `k8s/jobs/base/` paths. Pull latest changes.

---

## Next Steps

1. **Review** the Kustomize setup:
   ```bash
   kubectl kustomize k8s/jobs/overlays/production
   ```

2. **Deploy to development** for testing:
   ```bash
   kubectl apply -k k8s/jobs/overlays/dev
   ./scripts/import-water-year.sh 2024
   ```

3. **Deploy to production** when ready:
   ```bash
   kubectl apply -k k8s/jobs/overlays/production
   ```

4. **Customize for your needs**:
   - Edit overlay files to match your environment
   - Adjust image tags, resource limits, schedules
   - Create additional overlays if needed

---

## Reference

### Documentation
- [KUSTOMIZE.md](../k8s/jobs/KUSTOMIZE.md) - Detailed Kustomize guide
- [README.md](../k8s/jobs/README.md) - Job usage guide
- [Kustomize Official Docs](https://kustomize.io/)

### Files
- Base: `k8s/jobs/base/`
- Overlays: `k8s/jobs/overlays/{dev,staging,production}/`
- Scripts: `scripts/import-*.sh`, `scripts/setup-import-cronjobs.sh`

---

**Status**: ✅ Complete
**Version**: 1.0
**Last Updated**: 2025-10-27

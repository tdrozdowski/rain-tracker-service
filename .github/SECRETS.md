# GitHub Secrets Configuration

This document describes the optional GitHub secrets that can be configured for this repository.

## Optional Secrets

These secrets have default values and are **optional**. Only configure them if you need to override the defaults.

### Application Configuration

| Secret Name | Description | Default Value |
|-------------|-------------|---------------|
| `GAUGE_URL` | URL for fetching individual gauge readings | `https://alert.fcd.maricopa.gov/php/showdata4.php?ID=59700&NM=1000` |
| `GAUGE_LIST_URL` | URL for fetching gauge list/summary | `https://alert.fcd.maricopa.gov/alert/Rain/ev_rain.txt` |

## How to Configure Secrets

1. Go to your repository on GitHub
2. Click **Settings** → **Secrets and variables** → **Actions**
3. Click **New repository secret**
4. Enter the secret name and value
5. Click **Add secret**

## Environment Variables

The following environment variables are configured in the workflow and do **not** need to be set as secrets:

- `POSTGRES_USER`: postgres
- `POSTGRES_PASSWORD`: password
- `POSTGRES_DB`: rain_tracker
- `POSTGRES_DB_TEST`: rain_tracker_test
- `REGISTRY`: ghcr.io
- `IMAGE_NAME`: ${{ github.repository }}

## Automatic Secrets

These secrets are automatically provided by GitHub Actions:

- `GITHUB_TOKEN`: Used for authenticating to GitHub Container Registry (GHCR)
- `GITHUB_ACTOR`: Used as the username for GHCR login

## Notes

- The workflow uses the `||` operator to provide fallback values, so secrets are truly optional
- Example: `${{ secrets.GAUGE_URL || 'https://default-url.com' }}`
- This allows the workflow to run without any manual secret configuration

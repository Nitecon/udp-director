# GitHub Actions Workflows

This directory contains the CI/CD workflows for the UDP Director project.

## Workflows

### `ci.yml` - Continuous Integration
Runs on every push and pull request to `main` and `develop` branches.

**Jobs:**
- **Format Check**: Ensures code is formatted with `cargo fmt`
- **Clippy Lint**: Runs `cargo clippy` with strict warnings
- **Test**: Executes all unit tests
- **Build**: Builds release binary
- **Docker**: Validates Docker image builds

### `docker.yml` - Docker CI/CD
Builds and pushes Docker images to Docker Hub.

**Triggers:**
- Push to `main` branch
- Version tags (`v*.*.*`)
- Manual workflow dispatch

**Jobs:**
- **test**: Runs full test suite (fmt, clippy, tests)
- **build-and-push**: Builds and pushes Docker images

**Docker Tags:**
- `latest`: Always pushed on main branch commits
- `<version>`: Pushed when a version tag is created (e.g., `v1.0.0` → `1.0.0`)

**Required Secrets:**
- `DOCKERHUB_USERNAME`: Docker Hub username
- `DOCKERHUB_TOKEN`: Docker Hub access token

## Docker Hub Repository
Images are pushed to: `docker.io/nitecon/udp-director`

## Usage

### Triggering a Release
1. Create and push a version tag:
   ```bash
   git tag v1.0.0
   git push origin v1.0.0
   ```
2. The workflow will automatically build and push:
   - `nitecon/udp-director:1.0.0`
   - `nitecon/udp-director:latest`

### Manual Trigger
Navigate to Actions → Docker CI → Run workflow

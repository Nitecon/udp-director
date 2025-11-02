[← Back to README](../README.md)

# Docker Registry Configuration

## Registry Information

**Registry**: Docker Hub  
**Repository**: `nitecon/udp-director`  
**URL**: https://hub.docker.com/r/nitecon/udp-director  
**Current Status**: Production

## Automated Builds (Recommended)

Images are automatically built and pushed to Docker Hub via GitHub Actions when:
- **Push to `main` branch**: Creates `nitecon/udp-director:latest`
- **Version tags** (e.g., `v1.0.0`): Creates both `nitecon/udp-director:1.0.0` and `nitecon/udp-director:latest`

### Triggering a Release

```bash
# Create and push a version tag
git tag v1.0.0
git push origin v1.0.0

# GitHub Actions will automatically:
# 1. Run tests (fmt, clippy, cargo test)
# 2. Build Docker image
# 3. Push to nitecon/udp-director:1.0.0
# 4. Update nitecon/udp-director:latest
```

## Manual Build and Push

For local testing or custom builds:

```bash
# Login to Docker Hub
docker login

# Build
docker build -t nitecon/udp-director:latest .

# Tag with version (optional)
docker tag nitecon/udp-director:latest nitecon/udp-director:v0.1.0

# Push
docker push nitecon/udp-director:latest
docker push nitecon/udp-director:v0.1.0
```

## Docker Hub Authentication

For pushing images (maintainers only):

```bash
# Login to Docker Hub
docker login

# Enter your Docker Hub credentials
```

### GitHub Actions Secrets

The following secrets are configured for automated builds:

- `DOCKERHUB_USERNAME`: Docker Hub username
- `DOCKERHUB_TOKEN`: Docker Hub access token (not password)

To create a Docker Hub access token:
1. Go to https://hub.docker.com/settings/security
2. Click "New Access Token"
3. Give it a name (e.g., "GitHub Actions")
4. Copy the token and add it to GitHub repository secrets

## Kubernetes Image Pull

The deployment is configured to pull from Docker Hub:

```yaml
spec:
  containers:
    - name: udp-director
      image: nitecon/udp-director:latest
      imagePullPolicy: Always
```

### Public Images

Docker Hub images are **public** and do not require authentication to pull. Kubernetes can pull them directly without image pull secrets.

### Using Specific Versions

```bash
# Update to a specific version
kubectl set image deployment/udp-director \
  udp-director=nitecon/udp-director:1.0.0 \
  -n udp-director

# Rollback to previous version
kubectl rollout undo deployment/udp-director -n udp-director

# Check rollout status
kubectl rollout status deployment/udp-director -n udp-director
```

## Version Tagging Strategy

### Development
- **`latest`**: Automatically updated on every push to `main` branch
- Use for development and testing

### Release Candidates
- **`1.0.0-rc1`**: Create tag `v1.0.0-rc1`
- Use semantic versioning with `-rc` suffix

### Production Releases
- **`1.0.0`**: Create tag `v1.0.0`
- Use semantic versioning (MAJOR.MINOR.PATCH)
- Both versioned tag and `latest` are updated

### Tagging Best Practices

```bash
# For a new feature release
git tag v1.1.0
git push origin v1.1.0

# For a patch/bugfix
git tag v1.0.1
git push origin v1.0.1

# For a release candidate
git tag v2.0.0-rc1
git push origin v2.0.0-rc1
```

## Verifying Images

```bash
# Check available tags on Docker Hub
curl -s https://hub.docker.com/v2/repositories/nitecon/udp-director/tags/ | jq '.results[].name'

# Pull from Docker Hub
docker pull nitecon/udp-director:latest
docker pull nitecon/udp-director:1.0.0

# Check image details
docker inspect nitecon/udp-director:latest

# View image layers and size
docker history nitecon/udp-director:latest
```

## Troubleshooting

### "Image pull backoff" in Kubernetes

Check if the image exists on Docker Hub:
```bash
docker pull nitecon/udp-director:latest
```

Check pod events:
```bash
kubectl describe pod -n udp-director <pod-name>
```

Verify the image name in deployment:
```bash
kubectl get deployment udp-director -n udp-director -o jsonpath='{.spec.template.spec.containers[0].image}'
```

### GitHub Actions build fails

Check the Actions tab in GitHub:
1. Go to repository → Actions
2. Click on the failed workflow
3. Review logs for errors

Common issues:
- Tests failing (fmt, clippy, cargo test)
- Docker Hub credentials not configured
- Build context issues

### Manual push fails

Ensure you're logged in:
```bash
docker login
```

Check Docker daemon is running:
```bash
docker ps
```

Verify you have push permissions to the repository.

## Available Images

### Latest Development
```bash
docker pull nitecon/udp-director:latest
```

### Specific Versions
```bash
# Check available versions at:
# https://hub.docker.com/r/nitecon/udp-director/tags

docker pull nitecon/udp-director:1.0.0
docker pull nitecon/udp-director:1.0.1
```

## Image Maintenance

### Cleaning Local Images

```bash
# Remove old local images
docker image prune -a

# Remove specific version
docker rmi nitecon/udp-director:1.0.0

# Remove all udp-director images
docker images | grep nitecon/udp-director | awk '{print $3}' | xargs docker rmi
```

### Managing Docker Hub Images

Docker Hub images can be managed through the web interface:

1. Go to https://hub.docker.com/r/nitecon/udp-director/tags
2. Click on a tag to view details
3. Delete old tags if needed (maintainers only)

**Note**: Keep at least the last 3-5 versions for rollback purposes.

## CI/CD Integration

The project uses GitHub Actions for automated builds. See `.github/workflows/docker.yml` for the complete workflow.

### Workflow Overview

```yaml
# Triggered on:
# - Push to main branch
# - Version tags (v*.*.*)
# - Manual workflow dispatch

# Steps:
# 1. Run tests (fmt, clippy, cargo test)
# 2. Build Docker image with BuildKit
# 3. Push to Docker Hub
# 4. Tag appropriately (latest and/or version)
```

### Workflow Features

- ✅ Automated testing before build
- ✅ Multi-platform support (amd64)
- ✅ Build caching for faster builds
- ✅ SBOM and provenance generation
- ✅ Automatic tagging based on git tags

## Configuration Summary

- **Registry**: Docker Hub (docker.io)
- **Repository**: nitecon/udp-director
- **Default Tag**: latest
- **Pull Policy**: Always
- **Authentication**: Public (no auth required for pull)
- **Automated Builds**: Yes (GitHub Actions)
- **Status**: Production

## Quick Reference

```bash
# Pull latest
docker pull nitecon/udp-director:latest

# Pull specific version
docker pull nitecon/udp-director:1.0.0

# Run locally
docker run -p 9000:9000 -p 7777:7777/udp -p 9090:9090 \
  -v $(pwd)/config.yaml:/etc/udp-director/config.yaml \
  nitecon/udp-director:latest

# Deploy to Kubernetes
kubectl set image deployment/udp-director \
  udp-director=nitecon/udp-director:latest \
  -n udp-director
```

---

For questions or issues, open an issue on GitHub.

# Docker Registry Configuration

## Registry Information

**Registry URL**: `registry.nitecon.net`  
**Image Name**: `udp-director`  
**Current Status**: Testing/Development

## Building and Pushing

### Quick Start

```bash
# Build and push with 'latest' tag
./dockerpush.sh

# Build and push with specific version
./dockerpush.sh v0.1.0

# Using make
make docker-push
```

### What the Script Does

1. Builds the Docker image from the Dockerfile
2. Tags it with your registry URL
3. Pushes to `registry.nitecon.net/udp-director:latest`
4. If a version is specified, also tags and pushes that version

### Manual Build and Push

If you need to do it manually:

```bash
# Build
docker build -t registry.nitecon.net/udp-director:latest .

# Tag with version (optional)
docker tag registry.nitecon.net/udp-director:latest registry.nitecon.net/udp-director:v0.1.0

# Push
docker push registry.nitecon.net/udp-director:latest
docker push registry.nitecon.net/udp-director:v0.1.0
```

## Registry Authentication

If the registry requires authentication:

```bash
# Login to registry
docker login registry.nitecon.net

# Enter credentials when prompted
```

## Kubernetes Image Pull

The deployment is configured to pull from the registry:

```yaml
spec:
  containers:
    - name: udp-director
      image: registry.nitecon.net/udp-director:latest
      imagePullPolicy: Always
```

### Image Pull Secrets (if needed)

If your registry requires authentication in Kubernetes:

```bash
# Create image pull secret
kubectl create secret docker-registry nitecon-registry \
  --docker-server=registry.nitecon.net \
  --docker-username=<username> \
  --docker-password=<password> \
  --docker-email=<email> \
  -n udp-director

# Add to deployment
kubectl patch serviceaccount udp-director \
  -n udp-director \
  -p '{"imagePullSecrets": [{"name": "nitecon-registry"}]}'
```

Or add to the deployment.yaml:

```yaml
spec:
  serviceAccountName: udp-director
  imagePullSecrets:
    - name: nitecon-registry
  containers:
    - name: udp-director
      image: registry.nitecon.net/udp-director:latest
```

## Version Tagging Strategy

### Development
- Use `latest` tag for ongoing development
- `./dockerpush.sh` (defaults to latest)

### Testing
- Use semantic versioning with `-rc` suffix
- `./dockerpush.sh v0.1.0-rc1`

### Production
- Use semantic versioning
- `./dockerpush.sh v0.1.0`
- `./dockerpush.sh v0.2.0`

## Verifying the Push

```bash
# Check local images
docker images | grep udp-director

# Pull from registry to verify
docker pull registry.nitecon.net/udp-director:latest

# Check image details
docker inspect registry.nitecon.net/udp-director:latest
```

## Troubleshooting

### "Cannot connect to registry"

Check network connectivity:
```bash
ping registry.nitecon.net
curl -v https://registry.nitecon.net/v2/
```

### "Authentication required"

Login to the registry:
```bash
docker login registry.nitecon.net
```

### "Image pull backoff" in Kubernetes

Check if the image exists:
```bash
docker pull registry.nitecon.net/udp-director:latest
```

Check pod events:
```bash
kubectl describe pod -n udp-director <pod-name>
```

Verify image pull secrets are configured if needed.

### Build fails

Ensure you're in the project root:
```bash
cd /path/to/udp-director
./dockerpush.sh
```

Check Docker daemon is running:
```bash
docker ps
```

## Migration to Docker Hub

When ready to move to Docker Hub:

1. Update `dockerpush.sh`:
   ```bash
   REGISTRY="docker.io/yourusername"
   ```

2. Update `k8s/deployment.yaml`:
   ```yaml
   image: yourusername/udp-director:latest
   ```

3. Push to Docker Hub:
   ```bash
   docker login
   ./dockerpush.sh
   ```

## Registry Maintenance

### Cleaning Old Images

On your local machine:
```bash
# Remove old local images
docker image prune -a

# Remove specific version
docker rmi registry.nitecon.net/udp-director:v0.1.0
```

On the registry server (if you have access):
```bash
# List images
curl -X GET https://registry.nitecon.net/v2/udp-director/tags/list

# Delete specific tag (requires registry API access)
# Consult your registry documentation
```

## CI/CD Integration

For automated builds, add to your CI pipeline:

```yaml
# Example GitHub Actions
- name: Build and push
  run: |
    docker login registry.nitecon.net -u ${{ secrets.REGISTRY_USER }} -p ${{ secrets.REGISTRY_PASS }}
    ./dockerpush.sh ${{ github.ref_name }}
```

## Current Configuration Summary

- **Registry**: registry.nitecon.net
- **Image**: udp-director
- **Default Tag**: latest
- **Pull Policy**: Always
- **Authentication**: Configure if required
- **Purpose**: Testing and development

---

For questions or issues with the registry, contact your infrastructure team.

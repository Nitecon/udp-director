#!/bin/bash
set -e

# Docker registry configuration
REGISTRY="registry.nitecon.net"
IMAGE_NAME="udp-director"
VERSION="${1:-latest}"

# Full image tag
FULL_TAG="${REGISTRY}/${IMAGE_NAME}:${VERSION}"

echo "=========================================="
echo "Building and pushing UDP Director"
echo "=========================================="
echo "Registry: ${REGISTRY}"
echo "Image: ${IMAGE_NAME}"
echo "Version: ${VERSION}"
echo "Full tag: ${FULL_TAG}"
echo "=========================================="

# Build the Docker image
echo ""
echo "Step 1: Building Docker image..."
docker build -t ${FULL_TAG} .

# Also tag as latest if a specific version was provided
if [ "${VERSION}" != "latest" ]; then
    echo ""
    echo "Step 2: Tagging as latest..."
    docker tag ${FULL_TAG} ${REGISTRY}/${IMAGE_NAME}:latest
fi

# Push to registry
echo ""
echo "Step 3: Pushing to registry..."
docker push ${FULL_TAG}

if [ "${VERSION}" != "latest" ]; then
    echo ""
    echo "Step 4: Pushing latest tag..."
    docker push ${REGISTRY}/${IMAGE_NAME}:latest
fi

echo ""
echo "=========================================="
echo "✅ Successfully pushed to registry!"
echo "=========================================="
echo "Image: ${FULL_TAG}"
if [ "${VERSION}" != "latest" ]; then
    echo "Also tagged as: ${REGISTRY}/${IMAGE_NAME}:latest"
fi
echo ""
echo "To deploy to Kubernetes, update the image in k8s/deployment.yaml:"
echo "  image: ${FULL_TAG}"
echo ""
echo "Or use the latest tag:"
echo "  image: ${REGISTRY}/${IMAGE_NAME}:latest"
echo "=========================================="

#!/bin/bash
set -e

IMAGE="${CEREMONY_IMAGE:?Set CEREMONY_IMAGE to your container registry image}"

export DOCKER_BUILDKIT=1

# Build and push
docker build -t "$IMAGE" .
docker push "$IMAGE"

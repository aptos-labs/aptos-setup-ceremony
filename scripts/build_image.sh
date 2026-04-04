#!/bin/bash
set -e

IMAGE="us-central1-docker.pkg.dev/benchmark-zkid-circuit/aptos-setup-ceremony/server:latest"

export DOCKER_BUILDKIT=1

# Build and push
docker build -t $IMAGE .
docker push $IMAGE


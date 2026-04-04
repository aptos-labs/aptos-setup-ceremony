#!/bin/bash
set -e

IMAGE="us-central1-docker.pkg.dev/benchmark-zkid-circuit/aptos-setup-ceremony/server:latest"
INSTANCE="mine-central"

# Build and push
sudo docker build -t $IMAGE .
sudo docker push $IMAGE

# SSH to server and redeploy
gcloud compute ssh $INSTANCE --zone=$ZONE -- bash -s <<'EOF'
  cd aptos-setup-ceremony
  git pull
  sudo docker compose -f server.yml pull 
  sudo docker compose -f server.yml up -d 
  sudo docker image prune -f
EOF

#!/bin/bash

INSTANCE="mine-central"
ZONE="us-central1-a"

# SSH to server and redeploy
gcloud compute ssh $INSTANCE --zone=$ZONE --command "cd aptos-setup-ceremony; git pull; docker compose -f server.yml pull; docker compose -f server.yml up -d; docker image prune -f"

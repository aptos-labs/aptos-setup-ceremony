#!/bin/bash

set -e
INSTANCE="${CEREMONY_INSTANCE:?Set CEREMONY_INSTANCE to your VM name}"
ZONE="${CEREMONY_ZONE:?Set CEREMONY_ZONE to your VM zone}"

# SSH to server and redeploy
gcloud compute ssh "$INSTANCE" --zone="$ZONE" --command "cd ceremony; git pull; docker compose -f server.yml pull; docker compose -f server.yml up -d; docker image prune -f"

#!/bin/bash
# Temporary script to deploy topology with env vars

# Load environment variables
if [ -f ../.env ]; then
    export $(grep -v '^#' ../.env | xargs)
    echo "Loaded env vars from ../.env"
else
    echo "ERROR: ../.env not found"
    exit 1
fi

# Check required vars
if [ -z "$DITTO_APP_ID" ] || [ -z "$DITTO_OFFLINE_TOKEN" ] || [ -z "$DITTO_SHARED_KEY" ]; then
    echo "ERROR: Missing required Ditto environment variables"
    exit 1
fi

echo "DITTO_APP_ID: ${DITTO_APP_ID:0:20}..."
echo "Deploying topology..."

# Deploy
containerlab deploy -t topologies/platoon-24node-mesh-mode4.yaml

echo "Deployment complete!"

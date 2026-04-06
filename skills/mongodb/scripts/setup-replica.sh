#!/bin/bash
# Set up a MongoDB replica set for local development.
# Usage: ./setup-replica.sh [--port 27017] [--replset rs0]
#
# Requirements: mongod, mongosh

set -euo pipefail

PORT="${1:-27017}"
REPLSET="${2:-rs0}"
DATA_DIR="/tmp/mongo-${REPLSET}"

echo "=== MongoDB Replica Set Setup ==="
echo "Port: $PORT | Replica Set: $REPLSET | Data: $DATA_DIR"

mkdir -p "$DATA_DIR"

# Check if mongod is already running on this port
if lsof -i ":$PORT" &>/dev/null; then
    echo "Port $PORT is already in use. Is mongod already running?"
    exit 1
fi

echo "Starting mongod..."
mongod --replSet "$REPLSET" --port "$PORT" --dbpath "$DATA_DIR" --fork --logpath "$DATA_DIR/mongod.log"

echo "Initiating replica set..."
sleep 2
mongosh --port "$PORT" --eval "
  rs.initiate({
    _id: '$REPLSET',
    members: [{ _id: 0, host: 'localhost:$PORT' }]
  })
"

echo ""
echo "=== Replica set '$REPLSET' ready ==="
echo "Connection string: mongodb://localhost:$PORT/?replicaSet=$REPLSET"

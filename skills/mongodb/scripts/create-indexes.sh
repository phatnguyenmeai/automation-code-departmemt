#!/bin/bash
# Apply MongoDB indexes from a JSON definition file.
# Usage: ./create-indexes.sh <db-name> [index-file]
#
# Index file format (JSON):
# [
#   { "collection": "users", "keys": { "email": 1 }, "options": { "unique": true } },
#   { "collection": "orders", "keys": { "user_id": 1, "created_at": -1 }, "options": {} }
# ]

set -euo pipefail

DB_NAME="${1:?Usage: create-indexes.sh <db-name> [index-file]}"
INDEX_FILE="${2:-indexes.json}"

if [ ! -f "$INDEX_FILE" ]; then
    echo "Error: Index file '$INDEX_FILE' not found"
    exit 1
fi

echo "=== Creating MongoDB Indexes ==="
echo "Database: $DB_NAME | File: $INDEX_FILE"

mongosh "$DB_NAME" --eval "
  const indexes = $(cat "$INDEX_FILE");
  for (const idx of indexes) {
    print('Creating index on ' + idx.collection + ': ' + JSON.stringify(idx.keys));
    db[idx.collection].createIndex(idx.keys, idx.options || {});
  }
  print('Done: ' + indexes.length + ' indexes created');
"

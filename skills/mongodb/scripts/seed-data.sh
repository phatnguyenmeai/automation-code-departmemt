#!/bin/bash
# Seed MongoDB with test data from JSON files.
# Usage: ./seed-data.sh <db-name> <seed-dir>
#
# Each JSON file in seed-dir is imported into a collection
# named after the file (e.g., users.json → users collection).

set -euo pipefail

DB_NAME="${1:?Usage: seed-data.sh <db-name> <seed-dir>}"
SEED_DIR="${2:?Usage: seed-data.sh <db-name> <seed-dir>}"

if [ ! -d "$SEED_DIR" ]; then
    echo "Error: Seed directory '$SEED_DIR' not found"
    exit 1
fi

echo "=== Seeding MongoDB ==="
echo "Database: $DB_NAME | Seeds: $SEED_DIR"

for file in "$SEED_DIR"/*.json; do
    COLLECTION=$(basename "$file" .json)
    echo "Importing $file → $DB_NAME.$COLLECTION"
    mongoimport --db "$DB_NAME" --collection "$COLLECTION" --file "$file" --jsonArray --drop
done

echo "=== Seeding complete ==="

#!/bin/bash
# Generate a sprint plan from a requirements file.
# Usage: ./generate-sprint.sh <requirements.json> [sprint-duration-days]
#
# Input: JSON file with { "requirements": [...] }
# Output: Sprint plan JSON to stdout

set -euo pipefail

REQ_FILE="${1:?Usage: generate-sprint.sh <requirements.json> [sprint-duration-days]}"
DURATION="${2:-10}"

if [ ! -f "$REQ_FILE" ]; then
    echo "Error: Requirements file '$REQ_FILE' not found" >&2
    exit 1
fi

REQUIREMENTS=$(cat "$REQ_FILE")
SPRINT_ID="Sprint-$(date +%Y%m%d)"

cat << EOF
{
  "sprint": {
    "id": "$SPRINT_ID",
    "duration_days": $DURATION,
    "start_date": "$(date -I)",
    "end_date": "$(date -I -d "+${DURATION} days" 2>/dev/null || date -v+${DURATION}d -I)",
    "requirements": $REQUIREMENTS,
    "status": "planning",
    "capacity": {
      "dev": { "available_points": $(( DURATION * 2 )), "assigned_points": 0 },
      "frontend": { "available_points": $(( DURATION * 2 )), "assigned_points": 0 },
      "test": { "available_points": $(( DURATION * 1 )), "assigned_points": 0 }
    }
  }
}
EOF

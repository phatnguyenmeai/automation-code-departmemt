#!/bin/bash
# Validate user stories JSON against quality checks.
# Usage: ./validate-stories.sh <stories.json>
#
# Checks:
#   - Each story has id, title, as_a, i_want, so_that
#   - Each story has at least one acceptance criterion
#   - AC uses given/when/then format
#   - Priority is set (P0/P1/P2/P3)

set -euo pipefail

STORIES_FILE="${1:?Usage: validate-stories.sh <stories.json>}"

if [ ! -f "$STORIES_FILE" ]; then
    echo "Error: File '$STORIES_FILE' not found"
    exit 1
fi

echo "=== Validating User Stories ==="
echo "File: $STORIES_FILE"
echo ""

ERRORS=0

# Use python3 for JSON validation (available on most systems)
python3 << PYEOF
import json, sys

with open("$STORIES_FILE") as f:
    data = json.load(f)

stories = data.get("stories", data) if isinstance(data, dict) else data
if not isinstance(stories, list):
    print("ERROR: Expected 'stories' array")
    sys.exit(1)

errors = 0
for i, story in enumerate(stories):
    sid = story.get("id", f"story[{i}]")

    # Required fields
    for field in ["id", "title", "as_a", "i_want", "so_that"]:
        if not story.get(field):
            print(f"  ERROR [{sid}]: missing required field '{field}'")
            errors += 1

    # Acceptance criteria
    ac = story.get("acceptance_criteria", [])
    if not ac:
        print(f"  ERROR [{sid}]: no acceptance criteria")
        errors += 1
    else:
        for j, criterion in enumerate(ac):
            if isinstance(criterion, dict):
                for key in ["given", "when", "then"]:
                    if not criterion.get(key):
                        print(f"  WARN [{sid}] AC[{j}]: missing '{key}' in Given/When/Then")
            elif isinstance(criterion, str):
                has_gwt = any(kw in criterion.lower() for kw in ["given", "when", "then"])
                if not has_gwt:
                    print(f"  WARN [{sid}] AC[{j}]: not in Given/When/Then format")

    # Priority
    if not story.get("priority"):
        print(f"  WARN [{sid}]: no priority set")

print(f"\nValidation complete: {len(stories)} stories, {errors} errors")
sys.exit(1 if errors > 0 else 0)
PYEOF

exit $?

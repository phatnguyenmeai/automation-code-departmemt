#!/bin/bash
# Extract git diff for code review.
# Usage: ./review-diff.sh [base-branch]
#
# Outputs the diff between current branch and base branch (default: main).

set -euo pipefail

BASE="${1:-main}"

echo "=== Code Review Diff ==="
echo "Base: $BASE | Head: $(git rev-parse --abbrev-ref HEAD)"
echo ""

# Show changed files
echo "--- Changed Files ---"
git diff --name-status "$BASE"...HEAD
echo ""

# Show full diff
echo "--- Full Diff ---"
git diff "$BASE"...HEAD

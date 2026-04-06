#!/bin/bash
# Run Playwright E2E tests with configurable options.
# Usage: ./run-tests.sh [options]
#
# Options:
#   --project <name>    Browser project (chromium|firefox|mobile) [default: chromium]
#   --tag <tag>         Run only tests with this tag (@smoke, @auth, @crud)
#   --workers <n>       Number of parallel workers [default: 4]
#   --retries <n>       Number of retries on failure [default: 0]
#   --headed            Run in headed mode (visible browser)
#   --debug             Run with Playwright Inspector
#   --ci                Run in CI mode (retries=2, workers=4, all projects)

set -euo pipefail

PROJECT="chromium"
TAG=""
WORKERS=""
RETRIES=""
HEADED=""
DEBUG=""
CI=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --project) PROJECT="$2"; shift 2 ;;
        --tag) TAG="$2"; shift 2 ;;
        --workers) WORKERS="--workers=$2"; shift 2 ;;
        --retries) RETRIES="--retries=$2"; shift 2 ;;
        --headed) HEADED="--headed"; shift ;;
        --debug) DEBUG="--debug"; shift ;;
        --ci) CI="true"; shift ;;
        *) echo "Unknown option: $1"; exit 1 ;;
    esac
done

echo "=== Playwright E2E Tests ==="

# Install browsers if needed
if [ ! -d "node_modules" ]; then
    echo "Installing dependencies..."
    npm ci
fi

npx playwright install --with-deps "$PROJECT" 2>/dev/null || true

CMD="npx playwright test"

if [ -n "$CI" ]; then
    CMD="$CMD --retries=2 --workers=4 --reporter=json,html,github"
else
    CMD="$CMD --project=$PROJECT $WORKERS $RETRIES $HEADED $DEBUG"
    if [ -n "$TAG" ]; then
        CMD="$CMD --grep $TAG"
    fi
fi

echo "Running: $CMD"
eval "$CMD"
EXIT_CODE=$?

# Generate summary
if [ -f "test-results/results.json" ]; then
    echo ""
    echo "--- Test Summary ---"
    python3 -c "
import json
with open('test-results/results.json') as f:
    data = json.load(f)
suites = data.get('suites', [])
total = passed = failed = skipped = 0
for suite in suites:
    for spec in suite.get('specs', []):
        for test in spec.get('tests', []):
            total += 1
            status = test.get('status', 'unknown')
            if status == 'expected': passed += 1
            elif status == 'unexpected': failed += 1
            elif status == 'skipped': skipped += 1
print(f'Total: {total} | Passed: {passed} | Failed: {failed} | Skipped: {skipped}')
" 2>/dev/null || true
fi

exit $EXIT_CODE

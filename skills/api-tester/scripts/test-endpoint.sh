#!/bin/bash
# Quick API endpoint tester.
# Usage: ./test-endpoint.sh <method> <url> [body]
#
# Examples:
#   ./test-endpoint.sh GET http://localhost:3000/api/v1/users
#   ./test-endpoint.sh POST http://localhost:3000/api/v1/users '{"name":"John"}'

set -euo pipefail

METHOD="${1:?Usage: test-endpoint.sh <method> <url> [body]}"
URL="${2:?Usage: test-endpoint.sh <method> <url> [body]}"
BODY="${3:-}"

echo "=== API Test ==="
echo "$METHOD $URL"

if [ -n "$BODY" ]; then
    echo "Body: $BODY"
    RESPONSE=$(curl -s -w "\n---HTTP_STATUS:%{http_code}---" \
        -X "$METHOD" \
        -H "Content-Type: application/json" \
        -d "$BODY" \
        "$URL")
else
    RESPONSE=$(curl -s -w "\n---HTTP_STATUS:%{http_code}---" \
        -X "$METHOD" \
        "$URL")
fi

HTTP_STATUS=$(echo "$RESPONSE" | grep -o "HTTP_STATUS:[0-9]*" | cut -d: -f2)
BODY_RESPONSE=$(echo "$RESPONSE" | sed 's/---HTTP_STATUS:[0-9]*---//')

echo ""
echo "Status: $HTTP_STATUS"
echo "Response:"
echo "$BODY_RESPONSE" | python3 -m json.tool 2>/dev/null || echo "$BODY_RESPONSE"

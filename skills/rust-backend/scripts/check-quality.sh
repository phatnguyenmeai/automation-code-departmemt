#!/bin/bash
# Run Rust quality checks: fmt, clippy, test, audit.
# Usage: ./check-quality.sh [crate-name]
#
# If crate-name is provided, checks only that crate.
# Otherwise checks the entire workspace.

set -euo pipefail

CRATE="${1:-}"
SCOPE=""
if [ -n "$CRATE" ]; then
    SCOPE="-p $CRATE"
fi

echo "=== Rust Quality Check ==="

echo ""
echo "--- cargo fmt --check ---"
cargo fmt -- --check $SCOPE 2>&1 || {
    echo "FAIL: formatting issues found. Run 'cargo fmt' to fix."
    exit 1
}
echo "PASS: formatting OK"

echo ""
echo "--- cargo clippy ---"
cargo clippy $SCOPE -- -W clippy::all -D warnings 2>&1 || {
    echo "FAIL: clippy warnings found."
    exit 1
}
echo "PASS: clippy clean"

echo ""
echo "--- cargo test ---"
cargo test $SCOPE 2>&1 || {
    echo "FAIL: tests failed."
    exit 1
}
echo "PASS: all tests passed"

echo ""
echo "--- cargo audit (if available) ---"
if command -v cargo-audit &> /dev/null; then
    cargo audit 2>&1 || echo "WARN: audit found vulnerabilities"
else
    echo "SKIP: cargo-audit not installed (install with: cargo install cargo-audit)"
fi

echo ""
echo "=== All checks passed ==="

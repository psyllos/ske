#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [[ "${1:-}" == "--selftest" ]]; then
    exec python3 scripts/check-deps.py --selftest
fi
if [[ $# -ne 0 ]]; then
    printf 'usage: %s [--selftest]\n' "$0" >&2
    exit 2
fi
metadata=$(mktemp)
trap 'rm -f "$metadata"' EXIT
cargo metadata --format-version 1 > "$metadata"
python3 scripts/check-deps.py "$metadata"

#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
# prost-build runs with the vendored protoc selected by ske-cir/build.rs.
# Generation stays in OUT_DIR, never using the system protoc.
cargo build -p ske-cir
printf 'Rust protobuf generated with pinned vendored protoc\n'
if [[ -d adapters/go/gen ]]; then
    ./scripts/gen-proto-go.sh
    printf 'Go protobuf generated with pinned compiler and generator\n'
else
    printf 'SKIP Go protobuf (not committed in v0.1; no optional system generator invoked)\n'
fi

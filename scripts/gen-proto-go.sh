#!/bin/sh
set -eu
ROOT=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
cd "$ROOT"
mkdir -p target/tools adapters/go/gen
GENERATOR="$ROOT/target/tools/protoc-gen-go"
if [ ! -x "$GENERATOR" ] || [ "$("$GENERATOR" --version)" != "protoc-gen-go v1.36.11" ]; then
  GOBIN="$ROOT/target/tools" go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.36.11
fi
PROTOC=$(cargo run --quiet -p ske-cir --example protoc_path)
"$PROTOC" -I schemas --plugin=protoc-gen-go="$GENERATOR" \
  --go_out=adapters/go/gen --go_opt=module=ske/adapters/go/gen \
  --go_opt=Mcir/v1/cir.proto=ske/adapters/go/gen/cir/v1 \
  --go_opt=Madapter/v1/adapter.proto=ske/adapters/go/gen/adapter/v1 \
  schemas/cir/v1/cir.proto schemas/adapter/v1/adapter.proto

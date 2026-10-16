#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/.."
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
./scripts/check-deps.sh
./scripts/check-deps.sh --selftest
./scripts/gen-proto.sh
git diff --exit-code -- schemas/ adapters/go/gen/
RUSTFLAGS="-D warnings" cargo build --workspace --all-targets
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
SKE=target/debug/ske
"$SKE" init --db "$work/init.sqlite"
python3 - "$work/init.sqlite" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
tables = db.execute("SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'").fetchone()[0]
triggers = db.execute("SELECT count(*) FROM sqlite_master WHERE type='trigger'").fetchone()[0]
assert (tables, triggers) == (50, 21), (tables, triggers)
PY
cargo run -p ske-cir --example gen_fixtures -- --negative --out "$work/fx"
diff -r "$work/fx" tests/fixtures/cir
cargo test -p ske-cir --example gen_fixtures
for fixture in decide check adjusted switch3 invoke mem_nocall mem_opaque; do
    "$SKE" ingest cir --db "$work/$fixture.sqlite" --input "tests/fixtures/cir/$fixture.cir.pb" \
        --source-root examples/go-branch-lab --format json > "$work/$fixture.json"
    "$SKE" db check --db "$work/$fixture.sqlite" --format json > "$work/$fixture.check.json"
done
# These integration tests independently encode every §15.2.2 count, each
# negative error/rollback, and hand-authored projections of public JSON.
cargo test -p ske-cli --test counts
cargo test -p ske-cli --test negative
cargo test -p ske-cli --test golden
# v0.2: build and execute only our trusted adapter. Studied code is loaded as
# source; no build/test/generate command is delegated to its project.
if ! command -v go >/dev/null 2>&1; then
    printf 'Go is required to validate the v0.2 workspace\n' >&2
    exit 2
fi
(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./...)
(cd adapters/go && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)
"$SKE" ingest go --db "$work/go.sqlite" --repo examples/go-branch-lab \
    --adapter target/ske-adapter-go --yes-run-adapter --format json > "$work/go.json"
python3 - "$work/go.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as stream:
    result = json.load(stream)
assert result["status"] == "CLOSED" and result["inventory_status"] == "PARTIAL", result
assert result["diagnostics"]["ERROR"] == 0, result
assert {u["unit_key"]: u["inventory_status"] for u in result["units"]} == {
    "go:example@linux/amd64": "COMPLETE",
    "go:example/partial@linux/amd64": "PARTIAL",
    "go:example/tags@linux/amd64": "COMPLETE",
}, result
PY
"$SKE" db check --db "$work/go.sqlite" --format json > "$work/go.check.json"
cargo test --workspace
python3 scripts/validate-v03.py --verify-cases
"$SKE" ingest cir --db "$work/decide.sqlite" --input tests/fixtures/cir/decide.cir.pb \
    --source-root examples/go-branch-lab --format json > "$work/twice.json"
"$SKE" snapshots list --db "$work/decide.sqlite" --format json > "$work/snapshots.json"
python3 - "$work/decide.sqlite" <<'PY'
import sqlite3, sys
db = sqlite3.connect(sys.argv[1])
rows = db.execute("SELECT id, source_fingerprint, config_fingerprint FROM snapshots ORDER BY id").fetchall()
assert len(rows) == 2 and rows[0][0] != rows[1][0] and rows[0][1:] == rows[1][1:], rows
PY
# v0.5: behaviors require the Go adapter build; behavior tests exercise the
# full reconstruct -> rules -> evidence pipeline against the trusted adapter.
printf 'VERIFY OK v0.5\n'

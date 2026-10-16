#!/usr/bin/env python3
"""Run v0.2 acceptance with independent output assertions and retained logs."""
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
os.chdir(ROOT)
OUT = ROOT / "target" / "validation-v02"
OUT.mkdir(parents=True, exist_ok=True)
ENV = dict(os.environ, PATH=f"{ROOT / 'target/debug'}:{os.environ['PATH']}")
RESULTS = []

def expect(condition, message):
    assert condition, message


def run(number, command, assertion=None, named_test=False):
    started = time.monotonic()
    proc = subprocess.run(
        ["bash", "-o", "pipefail", "-c", command], env=ENV,
        stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, check=False,
    )
    passed, reason = proc.returncode == 0, ""
    if passed:
        try:
            if named_test:
                assert re.search(r"test result: ok\. [1-9][0-9]* passed", proc.stdout), "no named tests executed"
            if assertion:
                assertion(proc.stdout)
        except (AssertionError, KeyError, IndexError, TypeError, ValueError) as error:
            passed, reason = False, str(error)
    entry = {
        "ac": f"AC-0.2-{number:02d}", "status": "PASS" if passed else "FAIL",
        "command": command, "exit": proc.returncode, "assertion_error": reason,
        "seconds": round(time.monotonic() - started, 3),
    }
    RESULTS.append(entry)
    (OUT / f"{entry['ac']}.log").write_text(proc.stdout, encoding="utf-8")
    (OUT / "results.json").write_text(json.dumps(RESULTS, indent=2) + "\n", encoding="utf-8")
    print(f"{entry['ac']} {entry['status']} exit={proc.returncode} {reason}", flush=True)


def go_inventory(output):
    value = json.loads(output)
    assert value["status"] == "CLOSED" and value["inventory_status"] == "PARTIAL"
    assert value["diagnostics"]["ERROR"] == 0
    actual = {u["unit_key"]: u["inventory_status"] for u in value["units"]}
    assert actual == {
        "go:example@linux/amd64": "COMPLETE",
        "go:example/partial@linux/amd64": "PARTIAL",
        "go:example/tags@linux/amd64": "COMPLETE",
    }, actual


with tempfile.TemporaryDirectory(prefix="ske-v02-") as temp:
    db = shlex.quote(str(Path(temp) / "k.sqlite"))
    run(1, '(cd examples/go-branch-lab && go build ./... && go vet ./... && test -z "$(gofmt -l .)")')
    run(2, '(cd adapters/go && go vet ./... && test -z "$(gofmt -l .)" && go test ./...)')
    run(3, "(cd adapters/go && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)",
        lambda _: expect((ROOT / "target/ske-adapter-go").is_file(), "adapter binary missing"))
    run(4, "cargo test -p ske-ingest adapter_list_units", named_test=True)
    # Build our CLI explicitly; never execute a build of an untrusted project.
    subprocess.run(["cargo", "build", "-p", "ske-cli"], env=ENV, check=True)
    run(5, f"ske ingest go --db {db} --repo examples/go-branch-lab "
        "--adapter target/ske-adapter-go --yes-run-adapter --format json", go_inventory)
    run(6, "cargo test -p ske-cli --test go_golden_decisions", named_test=True)
    run(7, "(cd adapters/go && go test ./internal/decisions -run TestCounterMatchesEmitter) && "
        "cargo test -p ske-cli --test go_counters", named_test=True)
    for number, suite in [
        (8, "go_props"), (9, "go_diagnostics"), (10, "go_vs_cir"),
        (11, "go_build_tags"), (12, "go_deterministic"),
    ]:
        run(number, f"cargo test -p ske-cli --test {suite}", named_test=True)
    for number, package, test in [
        (13, "load", "TestEnvLocked"), (14, "load", "TestOnlyGoListInvocations"),
        (15, "frame", "TestRoundTripFixtures"),
    ]:
        # Verbose output distinguishes a real Go test from "no tests to run".
        run(number, f"(cd adapters/go && go test -v ./internal/{package} -run {test})",
            lambda text, name=test: expect(f"--- PASS: {name}" in text, f"{name} did not pass"))
    for number, command in [
        (16, "cargo test -p ske-cli --test go_failures"),
        (17, "cargo test -p ske-cli --test consent"),
        (18, "cargo test -p ske-ingest adapter_limits"),
        (19, "cargo test -p ske-ingest source_hash_"),
    ]:
        run(number, command, named_test=True)
    run(20, f"ske db check --db {db}")
    run(21, "git tag -l cir-v1-candidate",
        lambda text: expect(text.strip() == "cir-v1-candidate", "local CIR candidate tag missing"))
    run(22, "./scripts/verify.sh",
        lambda text: expect(text.rstrip().endswith(
            "VERIFY OK " + os.environ.get("SKE_EXPECT_VERIFY_VERSION", "v0.2")),
            "missing final expected VERIFY OK"))
    run(23, "SKE_EXPECT_VERIFY_VERSION=" + shlex.quote(
        os.environ.get("SKE_EXPECT_VERIFY_VERSION", "v0.2")) + " python3 scripts/validate-v01.py",
        lambda _: expect(all(r["status"] == "PASS" for r in json.loads(
            (ROOT / "target/validation-v01/results.json").read_text())), "v0.1 regression"))
    run(24, "cargo test -p ske-cli --test go_activation_cases", named_test=True)
    run(25, "cargo test -p ske-cli --test go_implicit_exit_inventory", named_test=True)

failures = [entry["ac"] for entry in RESULTS if entry["status"] != "PASS"]
print(json.dumps({"passed": len(RESULTS) - len(failures), "failed": failures, "logs": str(OUT)}, indent=2))
raise SystemExit(bool(failures))

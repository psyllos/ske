#!/usr/bin/env python3
"""Run v0.6 acceptance checks, never infer acceptance from an empty test filter."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target/validation-v06"
spec = importlib.util.spec_from_file_location("v05_runner", ROOT / "scripts/validate-v05.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
# Reuse logging, heartbeat and process-group cancellation, not inherited proof.
runner.ENV.pop("SKE_INHERIT_VERIFY", None)
runner.ENV["SKE_EXPECT_VERIFY_VERSION"] = "v0.6"
execute = runner.execute

FILTERS = {
    3: ("rust_golden_decisions",),
    4: ("java_golden_decisions",),
    5: ("lang_types",),
    6: ("cross_language_decide",),
    7: ("java_switch_ir", "java_invoke_exception"),
    8: ("java_reflection_unknown",),
    9: ("rust_toolchain_mismatch",),
    10: ("multilang_inspect",),
    11: ("multilang_integrity_framing",),
    14: ("java_multi_catch_finally",),
}
COUNTS = {"v01": 42, "v02": 25, "v03": 29, "v04": 14, "v05": 13}


def require_tests(text, name):
    passed = re.findall(r"^test ([\w:]+) \.\.\. ok$", text, re.M)
    assert any(name in p for p in passed), f"no passing test for filter {name}"
    assert re.search(r"test result: ok\. [1-9]\d* passed;", text), "zero actual tests"
    assert not re.search(r"\b[1-9]\d* ignored\b", text), "ignored tests"
    assert not re.search(r"^test result: FAILED", text, re.M), "failed tests"


def require_passes(records, numbers):
    statuses = {r["number"]: r["status"] for r in records}
    missing = [str(n) for n in numbers if statuses.get(n) != "PASS"]
    assert not missing, "prerequisites not PASS: " + ", ".join(missing)


def validate_regressions():
    for version, count in COUNTS.items():
        rows = json.loads((ROOT / f"target/validation-{version}/results.json").read_text())
        prefix = f"AC-0.{int(version[1:])}-"
        expected = {f"{prefix}{n:02}" for n in range(1, count + 1)}
        assert len(rows) == count, f"{version}: wrong count"
        assert {r["ac"] for r in rows} == expected, f"{version}: missing/duplicate AC"
        assert all(r["status"] == "PASS" for r in rows), f"{version}: not all PASS"


def commands(number):
    if number == 1:
        return ["(cd adapters/rust && cargo build)", "./scripts/check-deps.sh"]
    if number == 2:
        return ["(cd adapters/java && mvn -B verify)"]
    if number == 12:
        return ["./scripts/verify.sh", "python3 scripts/validate-v05.py"]
    if number == 13:
        return ["cargo test --workspace cir_compat_ -- --test-threads=1",
                "git tag -l cir-v1.0-frozen"]
    return [f"cargo test -p ske-cli --test multilang {name} -- --test-threads=1"
            for name in FILTERS[number]]


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", help="Comma-separated AC numbers; all others remain NOT_RUN")
    args = parser.parse_args(argv)
    try:
        selected = set(map(int, args.only.split(","))) if args.only is not None else set(range(1, 15))
        if not selected or not selected <= set(range(1, 15)):
            raise ValueError()
    except ValueError:
        parser.error("--only must contain AC numbers 1..14")
    OUT.mkdir(parents=True, exist_ok=True)
    records = [{"number": n, "ac": f"AC-0.6-{n:02}", "status": "NOT_RUN",
                "commands": commands(n), "executed_commands": []} for n in range(1, 15)]

    def persist():
        (OUT / "results.json").write_text(json.dumps(records, indent=2) + "\n")

    persist()
    # Regression precedes freeze; multi-catch must also pass before freeze.
    for n in [*range(1, 12), 14, 12, 13]:
        if n not in selected:
            continue
        record = records[n - 1]
        log = []
        try:
            if n in FILTERS:
                deps = [1] if n in (3, 9) else [2] if n in (4, 7, 8, 14) else [1, 2]
                require_passes(records, deps)
            if n == 12:
                require_passes(records, [*range(1, 12), 14])
                # Old results cannot serve as evidence of this invocation.
                for version in COUNTS:
                    (ROOT / f"target/validation-{version}/results.json").unlink(missing_ok=True)
            if n == 13:
                require_passes(records, [*range(1, 13), 14])
            for index, command in enumerate(record["commands"]):
                log.append("$ " + command + "\n")
                record["executed_commands"].append(command)
                persist()
                text = execute(command)
                log.append(text)
                if n in FILTERS:
                    require_tests(text, FILTERS[n][index])
                if n == 12 and index == 0:
                    assert text.rstrip().endswith("VERIFY OK v0.6"), "missing final VERIFY OK v0.6"
                if n == 13 and index == 0:
                    require_tests(text, "cir_compat_")
                if n == 13 and index == 1:
                    assert text.strip() == "cir-v1.0-frozen", "freeze tag absent"
            if n == 12:
                validate_regressions()
            record["status"] = "PASS"
        except (AssertionError, OSError, ValueError, KeyError) as error:
            record["status"] = "FAIL"
            record["error"] = str(error)
            log.append(str(error))
        (OUT / f"{record['ac']}.log").write_text("\n".join(log))
        persist()
        print(f"{record['ac']} {record['status']}", flush=True)
    return int(any(r["status"] != "PASS" for r in records))


if __name__ == "__main__":
    sys.exit(main())

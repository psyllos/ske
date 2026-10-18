#!/usr/bin/env python3
"""Run v0.7 acceptance checks; never infer acceptance from an empty filter."""
import argparse
import importlib.util
import json
from pathlib import Path
import re
import sys

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target/validation-v07"
spec = importlib.util.spec_from_file_location("v05_runner", ROOT / "scripts/validate-v05.py")
runner = importlib.util.module_from_spec(spec)
spec.loader.exec_module(runner)
# Reuse logging, heartbeat and process-group cancellation, not inherited proof.
runner.ENV.pop("SKE_INHERIT_VERIFY", None)
runner.ENV["SKE_EXPECT_VERIFY_VERSION"] = "v0.7"
execute = runner.execute

FILTERS = {
    1: ("dto_",),
    2: ("api_limits",),
    3: ("mcp_tools_list",),
    4: ("mcp_explain_decide",),
    5: ("mcp_impact_helper",),
    6: ("context_builder_",),
    7: ("no_llm_default",),
    8: ("llm_mock_provenance",),
    9: ("llm_no_source",),
}
# Per-AC test commands. The v0.7 ACs are tested in-process (ske-query dto_,
# ske-api api_limits/context_builder, ske-cli test files mcp.rs and llm.rs).
COMMANDS = {
    1: ["cargo test -p ske-query dto_ -- --test-threads=1",
        "test \"$(cargo tree -p ske-api --depth 1 -e normal | grep -c rusqlite || true)\" -eq 0"],
    2: ["cargo test -p ske-cli --test api_limits api_limits -- --test-threads=1"],
    3: ["cargo test -p ske-cli --test mcp mcp_tools_list -- --test-threads=1"],
    4: ["cargo test -p ske-cli --test mcp mcp_explain_decide -- --test-threads=1"],
    5: ["cargo test -p ske-cli --test mcp mcp_impact_helper -- --test-threads=1"],
    6: ["cargo test -p ske-api context_builder_ -- --test-threads=1"],
    7: ["cargo test -p ske-cli --test llm no_llm_default -- --test-threads=1"],
    8: ["cargo test -p ske-cli --test llm llm_mock_provenance -- --test-threads=1"],
    9: ["cargo test -p ske-cli --test llm llm_no_source -- --test-threads=1"],
    10: ["python3 scripts/ske-bench.py --check --out target/bench-v07-run$$"],
    # v0.6 runs verify.sh and regenerates all predecessors. Running v0.5
    # here cannot recreate the v0.6 result that we deliberately invalidate.
    11: ["SKE_EXPECT_VERIFY_VERSION=v0.7 python3 scripts/validate-v06.py"],
}
COUNTS = {"v01": 42, "v02": 25, "v03": 29, "v04": 14, "v05": 13, "v06": 14}



def require_tests(text, name):
    passed = re.findall(r"^test (\S+) \.\.\. ok$", text, re.M)
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


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", help="Comma-separated AC numbers; all others remain NOT_RUN")
    args = parser.parse_args(argv)
    try:
        selected = set(map(int, args.only.split(","))) if args.only is not None else set(range(1, 12))
        if not selected or not selected <= set(range(1, 12)):
            raise ValueError()
    except ValueError:
        parser.error("--only must contain AC numbers 1..11")
    OUT.mkdir(parents=True, exist_ok=True)
    records = [{"number": n, "ac": f"AC-0.7-{n:02}", "status": "NOT_RUN",
                "commands": COMMANDS[n], "executed_commands": []} for n in range(1, 12)]

    def persist():
        (OUT / "results.json").write_text(json.dumps(records, indent=2) + "\n")

    persist()
    for n in [*range(1, 11), 11]:
        if n not in selected:
            continue
        record = records[n - 1]
        log = []
        try:
            if n == 11:
                require_passes(records, list(range(1, 11)))
                for version in COUNTS:
                    (ROOT / f"target/validation-{version}/results.json").unlink(missing_ok=True)
            for index, command in enumerate(record["commands"]):
                log.append("$ " + command + "\n")
                record["executed_commands"].append(command)
                persist()
                text = execute(command)
                log.append(text)
                if n in FILTERS and index == 0:
                    require_tests(text, FILTERS[n][0])
                if n == 1 and index == 1:
                    assert text.strip() == "", f"ske-api declares rusqlite: {text}"
            if n == 11:
                validate_regressions()
            record["status"] = "PASS"
            record["checks_succeeded"] = True
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

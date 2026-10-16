#!/usr/bin/env python3
"""§24 v0.3 acceptance; real commands, strict test evidence, retained logs."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parent.parent
ENV = dict(os.environ, PATH=f"{ROOT / 'target/debug'}:{os.environ['PATH']}")
LAST_PROCESS = None


def require(condition, message):
    if not condition:
        raise AssertionError(message)


def tests(output, names=(), minimum=1):
    passed = re.findall(r"^test ([\w:]+) \.\.\. ok$", output, re.M)
    require(len(passed) >= minimum, "too few executed passing tests")
    require(not re.search(r"[1-9]\d* ignored", output), "ignored tests")
    for name in names:
        require(any(p == name or p.endswith("::" + name) for p in passed),
                f"required test did not execute: {name}")


def execute(command, expected=0):
    global LAST_PROCESS
    p = subprocess.run(["bash", "-o", "pipefail", "-c", command], cwd=ROOT,
                       env=ENV, capture_output=True, text=True, check=False)
    LAST_PROCESS = p
    require(p.returncode == expected,
            f"exit {p.returncode}, expected {expected}\n{p.stdout}{p.stderr}")
    return p.stdout


def test_command(command, names=(), minimum=1):
    output = execute(command)
    tests(output, names, minimum)
    return output


def verify_cases():
    # Real CLI + Z3 for SAT/accepted UNSAT, scripted backend only for the
    # nondeterministic solver failures; none of these messages precedes evidence.
    print(test_command("cargo test -p ske-cli --test analysis_table analysis_decide",
                       ["analysis_decide"]), end="")
    print("SAT case ok\nUNSAT case ok", flush=True)
    print(test_command("cargo test -p ske-symbolic -- scripted_unknown_",
                       ["scripted_unknown_continues_and_forbids_global_proofs"]), end="")
    print("UNKNOWN case ok", flush=True)
    print(test_command("cargo test -p ske-symbolic -- scripted_timeout_",
                       ["scripted_timeout_then_unsat_never_claims_closed"]), end="")
    print("TIMEOUT case ok", flush=True)
    print(test_command("cargo test -p ske-cli --test analysis_table analysis_sum_u3",
                       ["analysis_sum_u3"]), end="")
    print(test_command("cargo test -p ske-query solver_fail_preserves_validated_results",
                       ["solver_fail_preserves_validated_results"]), end="")
    print("PARTIAL never COMPLETE ok", flush=True)


def selftest():
    tests("test real ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;", ["real"])
    for output, names in [
        ("test result: ok. 0 passed; 0 failed; 0 ignored;", []),
        ("test other ... ok", ["missing"]),
        ("test real ... ok\n1 ignored", []),
    ]:
        try:
            tests(output, names)
        except AssertionError:
            continue
        raise AssertionError("false positive in test-evidence checker")
    print("VALIDATE v0.3 SELFTEST OK")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--selftest", action="store_true")
    parser.add_argument("--verify-cases", action="store_true")
    args = parser.parse_args()
    if args.selftest:
        selftest()
        return
    if args.verify_cases:
        verify_cases()
        return
    out = ROOT / "target/validation-v03"
    out.mkdir(parents=True, exist_ok=True)
    results = [{"ac": f"AC-0.3-{n:02d}", "status": "NOT_RUN"} for n in range(1, 30)]

    def run(n, command, check=None):
        started = time.monotonic()
        log = ""
        try:
            log = execute(command)
            if check:
                check(log)
            status, error = "PASS", ""
        except (AssertionError, ValueError, KeyError, TypeError, OSError) as e:
            status, error = "FAIL", str(e)
        results[n - 1] = dict(ac=f"AC-0.3-{n:02d}", status=status,
                             command=command, error=error,
                             exit=LAST_PROCESS.returncode if LAST_PROCESS else None,
                             seconds=round(time.monotonic() - started, 3))
        (out / f"AC-0.3-{n:02d}.log").write_text(
            log + "\n[stderr]\n" + (LAST_PROCESS.stderr if LAST_PROCESS else "") + "\n" + error)
        (out / "results.json").write_text(json.dumps(results, indent=2) + "\n")
        print(f"AC-0.3-{n:02d} {status} {error}", flush=True)

    def test(n, command, names=(), minimum=1):
        run(n, command, lambda text: tests(text, names, minimum))

    with tempfile.TemporaryDirectory(prefix="ske-v03-") as temp:
        db = shlex.quote(str(Path(temp) / "decide.sqlite"))
        run(1, "cargo test -p ske-solver z3_", tests)
        test(2, "cargo test -p ske-solver encoding_matches_bitvec", minimum=3)
        test(3, "cargo test -p ske-solver scripted_", ["scripted_order_and_text_preserved"])
        test(4, "cargo test -p ske-symbolic expr_", [
            "expr_interning_structural", "expr_simplify_each_rule", "expr_no_extra_simplification"])
        def summary(text):
            v = json.loads(text)
            require((v["run_status"], v["exploration_closed"], v["cut_count"]) ==
                    ("COMPLETED", True, 0), "Decide closure")
            require(v["counts"] == dict(UNANALYZED=0, POSSIBLE=3, WITNESSED=0,
                    PROVEN_UNREACHABLE=1, UNKNOWN=0, TIMEOUT=0, UNSUPPORTED=0), "Decide counts")
            require(set(v["assumptions"]) >= {"SEQUENTIAL_EXECUTION", "CLOSED_ENTRY_CONTEXT"},
                    "missing assumptions")
        run(5, f"cargo build -p ske-cli >&2 && ske ingest cir --db {db} "
            "--input tests/fixtures/cir/decide.cir.pb --source-root examples/go-branch-lab "
            f"--format json >/dev/null && ske branches analyze --db {db} "
            "--snapshot latest --entry example.Decide --format json",
            summary)
        def details(text):
            v = json.loads(text)
            require([o["status"] for o in v["ir_outcomes"]] ==
                    ["POSSIBLE", "POSSIBLE", "PROVEN_UNREACHABLE", "POSSIBLE"], "IR statuses")
            require(any(e["kind"] == "proof" for e in v["ir_outcomes"][2]["evidence"]),
                    "missing proof evidence")
            require([o["status"] for d in v["source_decisions"] for o in d["outcomes"]] ==
                    ["POSSIBLE", "POSSIBLE", "PROVEN_UNREACHABLE", "POSSIBLE"], "source statuses")
        run(6, f"ske branches list --db {db} --snapshot latest --function example.Decide "
            "--run latest --format json", details)
        test(7, "cargo test -p ske-verify decide_proof_row")
        table = ["decide", "abs", "adjusted", "classify", "check", "addone", "day",
                 "count3_u3", "count3_u2", "sum_u3", "mem_nocall", "mem_opaque",
                 "mem_alias", "mem_alloc", "hasa", "parse", "divmod"]
        test(8, "cargo test -p ske-cli --test analysis_table", ["analysis_" + t for t in table], 17)
        test(9, "cargo test -p ske-cli --test analysis_table analysis_addone", ["analysis_addone"])
        memory = ["memory_" + n for n in ["two_loads_same_expr", "strong_update",
                  "alias_param_param", "alloc_distinct", "havoc_epoch", "same_site_two_iterations",
                  "summary_never_strong_update", "alias_chain_soundness"]]
        test(10, "cargo test -p ske-symbolic memory_", memory)
        test(11, "cargo test -p ske-symbolic -- budget_ && cargo test -p ske-symbolic -- scripted_",
             ["budget_states_records_arrival_before_cut", "budget_queries_keeps_symbolic_successors",
              "scripted_unknown_continues_and_forbids_global_proofs",
              "scripted_timeout_then_unsat_never_claims_closed"])
        test(12, "cargo test -p ske-verify verifier_ && ./scripts/check-deps.sh",
             ["verifier_accepts_decide_proof_row", "verifier_rejects_scripted_false_unsat",
              "verifier_has_no_classification_dependency"])
        test(13, "cargo test -p ske-storage trigger_")
        test(14, "cargo test -p ske-query solver_fail_preserves_validated_results",
             ["solver_fail_preserves_validated_results"])
        test(15, "cargo test -p ske-cli --test analysis_table",
             ["analysis_count3_u3", "analysis_count3_u2", "analysis_sum_u3"], 17)
        test(16, "cargo test -p ske-cli --test analysis_table analysis_sum_u3", ["analysis_sum_u3"])
        test(17, "cargo test -p ske-cli --test cancel_sigint")
        test(18, f"ske runs list --db {db} --format json && cargo test -p ske-cli --test runs",
             ["runs_mark_failed"])
        test(19, "cargo test -p ske-cli --test analysis_deterministic")
        test(20, "cargo test --workspace prop_", [
            "prop_unsupported_never_exact", "prop_inventory_never_shrinks",
            "prop_json_possible_vs_witnessed",
            "prop_budget_diag", "prop_no_complete_on_error"])
        test(21, "cargo test -p ske-cli --test differential")
        def integrity(text):
            v = json.loads(text)
            require(v["ok"] and len(v["checks"]) == 22 and
                    all(c["violations"] == 0 for c in v["checks"]), "22 integrity checks")
        run(22, f"ske db check --db {db} --format json", integrity)
        def docs(_):
            text = (ROOT / "docs/soundness-and-limits.md").read_text().lower()
            for term in ["m1", "call", "boucle", "panic", "flottant", "chaîne"]:
                require(term in text, f"missing documented limitation {term}")
            require((ROOT / "docs/validation/v0.3.md").is_file(), "missing report")
        run(23, "test -f docs/soundness-and-limits.md && test -f docs/validation/v0.3.md", docs)
        def verify(text):
            require(text.rstrip().endswith("VERIFY OK v0.3"), "final verify line")
            for message in ["SAT case ok", "UNSAT case ok", "UNKNOWN case ok",
                            "TIMEOUT case ok", "PARTIAL never COMPLETE ok"]:
                require(message in text, f"missing {message}")
        run(24, "./scripts/verify.sh", verify)
        def regression(_):
            for version, count in [("v01", 42), ("v02", 25)]:
                rows = json.loads((ROOT / f"target/validation-{version}/results.json").read_text())
                require(len(rows) == count and all(r["status"] == "PASS" for r in rows),
                        f"{version} regression")
        run(25, "SKE_EXPECT_VERIFY_VERSION=v0.3 python3 scripts/validate-v02.py", regression)
        test(26, "cargo test -p ske-symbolic memory_ -- --list && cargo test -p ske-symbolic memory_", memory)
        test(27, "cargo test -p ske-cli --test source_activation_regression")
        test(28, "cargo test -p ske-symbolic call_not_referentially_transparent")
        test(29, "cargo test -p ske-cli --test verification_level")
    report = "# Validation v0.3\n\nGenerated by `python3 scripts/validate-v03.py`; raw logs in `target/validation-v03/`.\n\n"
    report += "| AC | Status | Command | Error |\n|---|---|---|---|\n"
    for r in results:
        report += f"| {r['ac']} | {r['status']} | `{r.get('command', '')}` | {r.get('error', '').replace(chr(10), ' ')} |\n"
    (ROOT / "docs/validation/v0.3.md").write_text(report)
    raise SystemExit(any(r["status"] != "PASS" for r in results))


if __name__ == "__main__":
    main()

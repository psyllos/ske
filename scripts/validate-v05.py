#!/usr/bin/env python3
"""Execute the thirteen v0.5 acceptance checks; retain commands and raw logs."""
import json
import argparse
import os
from pathlib import Path
import re
import subprocess
import sys
import signal
import threading
import time

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target/validation-v05"
ENV = dict(os.environ, PATH=f"{ROOT / 'target/debug'}:{os.environ['PATH']}")


def execute(command):
    start = time.monotonic()
    done = threading.Event()
    def heartbeat():
        while not done.wait(30):
            print(f"RUNNING {int(time.monotonic() - start)}s: {command}", flush=True)
    watcher = threading.Thread(target=heartbeat, daemon=True)
    process = subprocess.Popen(["bash", "-o", "pipefail", "-c", command],
                               cwd=ROOT, env=ENV, text=True,
                               stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                               start_new_session=True)
    watcher.start()
    chunks = []
    try:
        for line in process.stdout:
            chunks.append(line)
            print(line, end="", flush=True)
        code = process.wait()
    except BaseException:
        if process.poll() is None:
            try:
                os.killpg(process.pid, signal.SIGTERM)
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                os.killpg(process.pid, signal.SIGKILL)
                process.wait()
            except ProcessLookupError:
                pass
        raise
    finally:
        process.stdout.close()
        done.set()
        watcher.join()
    text = "".join(chunks)
    print(f"END exit={code} {time.monotonic() - start:.2f}s: {command}", flush=True)
    if code:
        raise AssertionError(f"exit {code}\n{text}")
    return text

def require_regression_ready(results):
    failed = [r["ac"] for r in results if r["status"] == "FAIL"]
    if failed:
        raise AssertionError("regression not started: prerequisite failures: " + ", ".join(failed))


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--only", help="Comma-separated AC numbers; others remain NOT_RUN")
    args = parser.parse_args()
    selected = set(map(int, args.only.split(","))) if args.only else set(range(1, 14))
    OUT.mkdir(parents=True, exist_ok=True)
    results = []

    def check(number, command, names=(), validator=None):
        record = {"ac": f"AC-0.5-{number:02}", "command": command}
        if number not in selected:
            record["status"] = "NOT_RUN"
            results.append(record)
            (OUT / "results.json").write_text(json.dumps(results, indent=2))
            return
        print(f"{record['ac']} RUNNING: {command}", flush=True)
        try:
            if number == 12:
                require_regression_ready(results)
                for version in ("v01", "v02", "v03", "v04"):
                    (ROOT / f"target/validation-{version}/results.json").unlink(missing_ok=True)
            text = execute(command)
            passed = re.findall(r"^test ([\w:]+) \.\.\. ok$", text, re.M)
            for name in names:
                assert any(p == name or p.endswith("::" + name) for p in passed), f"test not executed: {name}"
            assert not re.search(r"[1-9]\d* ignored", text), "ignored tests"
            if validator:
                validator(text)
            record["status"] = "PASS"
        except (AssertionError, OSError) as error:
            text = str(error)
            record["status"] = "FAIL"
            record["error"] = text
        (OUT / f"{record['ac']}.log").write_text(f"$ {command}\n{text}")
        results.append(record)
        (OUT / "results.json").write_text(json.dumps(results, indent=2))
        print(f"{record['ac']} {record['status']}", flush=True)

    def build_adapter(_):
        execute("cargo build -p ske-cli && (cd adapters/go && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)")

    check(1, "cargo test -p ske-behavior",
          ["render_guard_r1_conjunction_in_path_order", "render_guard_r2_negation_push_and_bare_bool",
           "render_guard_decide_negations_and_signed_constants", "render_guard_r6_adjusted_infix_without_simplification",
           "render_guard_r1_preserves_repeated_atoms", "render_r4_fallbacks_and_globals", "render_r3_signed_and_unsigned_constants",
           "behavior_case_ordering_by_span_then_ir_path", "behavior_case_dedup_conservative",
           "render_guard_r5_logical_grouping_and_r2_all_comparisons", "rule_description_returns_and_panic"])
    check(2, "cargo test -p ske-cli --test behavior behavior_check_exact_reference",
          ["behavior_check_exact_reference"])
    check(3, "cargo test -p ske-cli --test behavior behavior_check_rules_and_evidence_11",
          ["behavior_check_rules_and_evidence_11"])
    check(4, "cargo test -p ske-cli --test behavior behavior_decide_no_unreachable_return",
          ["behavior_decide_no_unreachable_return"], build_adapter)
    check(5, "cargo test -p ske-cli --test behavior behavior_adjusted_no_simplification && cargo test -p ske-cli --test behavior behavior_entry_reference && cargo test -p ske-cli --test behavior behavior_divmod_reference",
          ["behavior_adjusted_no_simplification", "behavior_entry_reference", "behavior_divmod_reference"])
    check(6, "cargo test -p ske-behavior behavior_case_dedup_conservative && cargo test -p ske-cli --test behavior behavior_dedup",
          ["behavior_case_dedup_conservative","behavior_dedup_retains_every_summary_and_result_site"])
    check(7, "cargo test -p ske-cli --test behavior behavior_unknown_effect_memopaque_overapprox",
          ["behavior_unknown_effect_memopaque_overapprox"])
    check(8, "cargo test -p ske-cli --test behavior behavior_no_business_text", ["behavior_no_business_text"])
    check(9, "cargo test -p ske-cli --test behavior summaries_reuse_across_snapshots",
          ["summaries_reuse_across_snapshots"])
    check(10, "cargo test -p ske-cli --test behavior rules_stale_check_detects_function_change",
           ["rules_stale_check_detects_function_change"])
    check(11, "cargo test -p ske-cli --test behavior behavior_yaml_parses_same_as_json",
           ["behavior_yaml_parses_same_as_json"])
    check(13, "cargo test -p ske-storage", ["migration_0002_capabilities", "schema_tables_match", "migrations_idempotent_wal_and_checksum"])

    def regression(text):
        expected_version = os.environ.get("SKE_EXPECT_VERIFY_VERSION", "v0.5")
        assert f"VERIFY OK {expected_version}" in text
        # Non-régression complète per AC-0.5-12: v0.1 (42), v0.2 (25), v0.3 (29),
        # v0.4 (14) all PASS.
        for version, expected in [("v01", 42), ("v02", 25), ("v03", 29), ("v04", 14)]:
            data = json.loads((ROOT / f"target/validation-{version}/results.json").read_text())
            assert len(data) == expected and all(r["status"] == "PASS" for r in data), version

    expected_version = os.environ.get("SKE_EXPECT_VERIFY_VERSION", "v0.5")
    if not re.fullmatch(r"v0\.[1-7]", expected_version):
        parser.error("SKE_EXPECT_VERIFY_VERSION must name v0.1 through v0.7")
    check(12, f"./scripts/verify.sh && SKE_INHERIT_VERIFY=1 SKE_EXPECT_VERIFY_VERSION={expected_version} python3 scripts/validate-v04.py",
          validator=regression)
    invocation = "python3 scripts/validate-v05.py" + (f" --only {args.only}" if args.only else "")
    rows = ["# Validation v0.5 — en cours", "", f"Invocation : `{invocation}`.",
            "Logs complets locaux : `target/validation-v05/AC-0.5-*.log`.",
            "NOT_RUN signifie non exécuté dans cette invocation, pas une dérogation.",
            "Le jalon reste non validé tant que tous les AC ne sont pas vérifiés et les",
            "limites ouvertes du journal ne sont pas résolues.", "",
            "| AC | Statut | Commande exécutée |", "|---|---|---|"]
    rows += [f"| {r['ac']} | {r['status']} | `{r['command']}` |" for r in results]
    rows += ["", "Les commandes ciblent le CLI réel / la façade Query avec l'adaptateur Go de confiance.",
             "Un test exécuté ne dispense pas de vérifier l'intégralité de son AC.",
             "Voir `docs/work-log.md` et `docs/soundness-and-limits.md` pour les limites ouvertes."]
    report = "\n".join(rows) + "\n"
    # Artefacts are always written under target/; the tracked report advances
    # only on explicit promotion. A campaign (including the nested regression
    # chain) never clobbers a hand-maintained "validée" framing: run with
    # SKE_PROMOTE_DOC=1 to update docs/validation/v0.5.md deliberately.
    (OUT / "report.md").write_text(report)
    if os.environ.get("SKE_PROMOTE_DOC") == "1":
        (ROOT / "docs/validation/v0.5.md").write_text(report)
    raise SystemExit(any(r["status"] != "PASS" for r in results))


if __name__ == "__main__":
    main()

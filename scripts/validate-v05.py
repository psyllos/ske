#!/usr/bin/env python3
"""Execute the thirteen v0.5 acceptance checks; retain commands and raw logs."""
import json
import argparse
import os
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target/validation-v05"
ENV = dict(os.environ, PATH=f"{ROOT / 'target/debug'}:{os.environ['PATH']}")


def execute(command):
    result = subprocess.run(["bash", "-o", "pipefail", "-c", command],
                            cwd=ROOT, env=ENV, text=True, capture_output=True)
    text = result.stdout + result.stderr
    if result.returncode:
        raise AssertionError(f"exit {result.returncode}\n{text}")
    return text


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
           "render_guard_r1_dedup_identical_atoms_divmod", "render_r4_fallbacks_and_globals", "render_r3_signed_and_unsigned_constants",
           "behavior_case_ordering_by_span_then_ir_path", "behavior_case_dedup_conservative", "rule_description_returns_and_panic"])
    check(2, "cargo test -p ske-cli --test behavior behavior_check_exact_reference",
          ["behavior_check_exact_reference"])
    check(3, "cargo test -p ske-cli --test behavior behavior_check_rules_and_evidence_11",
          ["behavior_check_rules_and_evidence_11"])
    check(4, "cargo test -p ske-cli --test behavior behavior_decide_no_unreachable_return",
          ["behavior_decide_no_unreachable_return"], build_adapter)
    check(5, "cargo test -p ske-cli --test behavior behavior_adjusted_no_simplification && cargo test -p ske-cli --test behavior behavior_entry_reference && cargo test -p ske-cli --test behavior behavior_divmod_reference",
          ["behavior_adjusted_no_simplification", "behavior_entry_reference", "behavior_divmod_reference"])
    check(6, "cargo test -p ske-behavior behavior_case_dedup_conservative", ["behavior_case_dedup_conservative"])
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
        assert "VERIFY OK v0.5" in text
        # Non-régression complète per AC-0.5-12: v0.1 (42), v0.2 (25), v0.3 (29),
        # v0.4 (14) all PASS.
        for version, expected in [("v01", 42), ("v02", 25), ("v03", 29), ("v04", 14)]:
            data = json.loads((ROOT / f"target/validation-{version}/results.json").read_text())
            assert len(data) == expected and all(r["status"] == "PASS" for r in data), version

    check(12, "./scripts/verify.sh && SKE_EXPECT_VERIFY_VERSION=v0.5 python3 scripts/validate-v04.py",
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
    (ROOT / "docs/validation/v0.5.md").write_text("\n".join(rows) + "\n")
    raise SystemExit(any(r["status"] != "PASS" for r in results))


if __name__ == "__main__":
    main()

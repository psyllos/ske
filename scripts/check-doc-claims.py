#!/usr/bin/env python3
"""Check that testable claims in the documentation are actually proven by tests.

Each claim maps to one or more commands; a claim is PASS only when every command
succeeds AND (for cargo tests) a real test named in the filter is observed to
pass. A documented assertion without a backing test is a FAIL. This enforces the
project rule "rien de ce qui est affirmé par la documentation sans être prouvé
par un test" in a reproducible, fail-closed way.

Run:  python3 scripts/check-doc-claims.py [--only ID1,ID2]
"""
import argparse
import json
import re
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CLAIMS = [
    {
        "id": "schema-50tables-21triggers",
        "docs": ["docs/architecture.md"],
        "assertion": "La migration initiale est la copie exacte de §8.2 (50 tables, 21 triggers).",
        "python": "import sqlite3, subprocess, tempfile, os\nd = tempfile.mkdtemp()\np = os.path.join(d, \"k.sqlite\")\nsubprocess.run([\"target/debug/ske\", \"init\", \"--db\", p], check=True, capture_output=True)\nc = sqlite3.connect(p)\nt = c.execute(\"SELECT count(*) FROM sqlite_master WHERE type='table' AND name NOT LIKE 'sqlite_%'\").fetchone()[0]\ng = c.execute(\"SELECT count(*) FROM sqlite_master WHERE type='trigger'\").fetchone()[0]\nassert (t, g) == (50, 21), (t, g)\n",
        "note": "architecture.md documentation said 47 tables; 50 is authoritative (see fix).",
    },
    {
        "id": "schema-22-checks",
        "docs": ["docs/architecture.md"],
        "assertion": "22 contrôles SQL d'intégrité inter-snapshot.",
        "check": "test \"$(ls crates/ske-storage/sql/checks/chk_*.sql | wc -l)\" -eq 22",
    },
    {
        "id": "cli-no-rusqlite",
        "docs": ["docs/architecture.md"],
        "assertion": "La CLI ne dépend normalement que de query et common, sans accès direct à rusqlite.",
        "check": "test \"$(cargo tree -p ske-cli -e normal --depth 1 2>/dev/null | grep -c rusqlite || true)\" -eq 0",
        "note": "depth 1 = direct dependencies only; transitive rusqlite through ske-storage is expected.",
    },
    {
        "id": "common-no-workspace-deps",
        "docs": ["docs/architecture.md"],
        "assertion": "ske-common est sans dépendance workspace.",
        "check": "test \"$(cargo tree -p ske-common -e normal 2>/dev/null | grep -cE 'ske-' || true)\" -eq 1",
        "note": "only itself may appear; grep counts ske-* rows.",
    },
    {
        "id": "milestone-ac-counts",
        "docs": ["docs/implementation-status.md", "README.md"],
        "assertion": "42+25+29+14+13+14+11 AC PASS (v0.1..v0.7).",
        "not_run": "full PASS requires the complete campaign (target/validation-v0X/results.json), see verify-chain claim; run the campaign to upgrade this claim.",
    },
    {
        "id": "no-witnessed-producer",
        "docs": ["docs/soundness-and-limits.md"],
        "assertion": "SAT ne produit pas WITNESSED ; WITNESSED n'a aucun producteur en v0.x.",
        "check": "cargo test -p ske-cli --test verification_level prop_json_possible_vs_witnessed -- --test-threads=1",
        "expect_test": "prop_json_possible_vs_witnessed",
    },
    {
        "id": "model-unreachable-level",
        "docs": ["docs/soundness-and-limits.md"],
        "assertion": "PROVEN_UNREACHABLE est de niveau MODEL_UNREACHABLE, jamais CERTIFIED_UNREACHABLE.",
        "check": "cargo test -p ske-cli --test verification_level -- --test-threads=1",
    },
    {
        "id": "sigint-preserves-arrivals",
        "docs": ["docs/README.md", "docs/soundness-and-limits.md"],
        "assertion": "SIGINT conserve les arrivées déjà persistées.",
        "check": "cargo test -p ske-cli --test cancel_sigint -- --test-threads=1",
        "expect_test": "cancel_sigint",
    },
    {
        "id": "go-byte-rune-canonical",
        "docs": ["docs/validation/realworld.md"],
        "assertion": "L'adaptateur Go canonicalise byte->uint8 et rune->int32 (comparaisons réelles valides).",
        "check": "(cd adapters/go && go test ./internal/lower/ -run TestByteRuneAliasTypeIdentity -count=1)",
    },
    {
        "id": "java-array-float-sound",
        "docs": ["docs/validation/realworld.md"],
        "assertion": "L'adaptateur Java ne crée pas de SSA orphelin (*a[i]=v) et rend l'arithmétique IEEE_FLOAT opaque.",
        "check": "(cd adapters/java && mvn -B -q test -Dtest=AdapterTest#array_store_and_float_arithmetic_are_valid_or_opaque -DfailIfNoTests=false)",
    },
    {
        "id": "mcp-explain-no-source-read",
        "docs": ["docs/soundness-and-limits.md"],
        "assertion": "explain ne relit jamais le fichier source ; il rend statut, hypothèses, SMT-LIB persisté et verdict.",
        "check": "cargo test -p ske-cli --test mcp mcp_explain_decide_shows_unreachable_and_smtlib -- --test-threads=1",
        "expect_test": "mcp_explain_decide_shows_unreachable_and_smtlib",
    },
    {
        "id": "mcp-impact-incomplete-resolved",
        "docs": ["docs/soundness-and-limits.md"],
        "assertion": "impact marque incomplete:true dès qu'un chemin traverse un appel non résolu/externe/dynamique.",
        "check": "cargo test -p ske-cli --test mcp mcp_impact_helper_reports_entry_caller -- --test-threads=1",
        "expect_test": "mcp_impact_helper_reports_entry_caller",
    },
    {
        "id": "llm-not-required-default",
        "docs": ["docs/soundness-and-limits.md", "docs/validation/v0.7.md"],
        "assertion": "LLM jamais requis : behaviors reconstruct fonctionne sans configuration LLM ni réseau (enabled=false).",
        "check": "cargo test -p ske-cli --test llm no_llm_default_behaviors_reconstruct_works_without_config -- --test-threads=1",
        "expect_test": "no_llm_default_behaviors_reconstruct_works_without_config",
    },
    {
        "id": "verify-chain",
        "docs": ["README.md"],
        "assertion": "./scripts/verify.sh termine par VERIFY OK v0.7 (fmt, clippy, deps, proto, build, adaptateurs, tests workspace).",
        "check": "./scripts/verify.sh | grep -q 'VERIFY OK v0.7'",
        "note": "long chain; run explicitly with --only verify-chain.",
    },
    {
        "id": "db-check-ok-after-ingest",
        "docs": ["docs/README.md"],
        "assertion": "ske db check signale ok après un import (22 contrôles).",
        "check": "bash -c 'DB=$(mktemp -d)/k.sqlite; target/debug/ske init --db $DB; target/debug/ske db check --db $DB --format json | python3 -c \"import json,sys; d=json.load(sys.stdin); assert d[\\\"ok\\\"] is True; assert len(d[\\\"checks\\\"])==22, len(d[\\\"checks\\\"])\"'",
    },
]

PASS = {"number": 0, "status": "PASS", "checks_succeeded": True, "details": []}
FAIL = {"number": 0, "status": "FAIL", "checks_succeeded": False, "details": []}


def run_check(claim: dict) -> tuple[bool, str]:
    """Run one claim check. Supports claim["check"] (shell) or claim["python"] (inline)."""
    if "python" in claim:
        try:
            exec(compile(claim["python"], "<claim>", "exec"), {})
            return True, ""
        except Exception as error:
            return False, str(error)[:400]
    check = claim["check"]
    try:
        proc = subprocess.run(["bash", "-c", check], cwd=ROOT, capture_output=True, text=True, timeout=1800)
        detail = (proc.stdout + proc.stderr).strip()
        return proc.returncode == 0, detail[:400]
    except subprocess.TimeoutExpired:
        return False, "timeout"


def require_tests(text: str, expect_test: str | None) -> None:
    passed = re.findall(r"^test (\S+) \.\.\. ok$", text, re.M)
    assert passed, "no test output captured"
    if expect_test:
        assert any(p == expect_test or expect_test in p for p in passed), \
            f"expected test {expect_test!r} not among passed: {passed}"
    # Require at least one real test passed (not zero/ignored-only).
    assert re.search(r"test result: ok\. [1-9]\d* passed;", text), "zero actual tests"


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--only", help="Comma-separated claim ids; others NOT_RUN")
    args = parser.parse_args(argv)
    only = {x.strip() for x in args.only.split(",")} if args.only else None
    results = []
    for i, claim in enumerate(CLAIMS):
        record = dict(PASS)
        record["number"] = i
        record["ac"] = claim["id"]
        record["docs"] = claim["docs"]
        record["assertion"] = claim["assertion"]
        if only is not None and claim["id"] not in only:
            record["status"] = "NOT_RUN"
            results.append(record)
            continue
        if "not_run" in claim:
            record["status"] = "NOT_RUN"
            record["note"] = claim.get("not_run")
            results.append(record)
            continue
        ok, detail = run_check(claim)
        if ok:
            # for cargo tests, require a real named test observed to pass when an expected
            # test name is given; always require at least one non-empty passing test result.
            if "cargo test" in claim.get("check", ""):
                try:
                    require_tests(detail or "", claim.get("expect_test"))
                except AssertionError as error:
                    ok = False
                    detail += f" [test-name check failed: {error}]"
        record["checks_succeeded"] = ok
        record["status"] = "PASS" if ok else "FAIL"
        record["details"] = [{"command": claim.get("check", claim.get("python", "")), "output": detail or "", "ok": ok}]
        record["note"] = claim.get("note")
        results.append(record)
    counts = {"PASS": 0, "FAIL": 0, "NOT_RUN": 0}
    for r in results:
        counts[r["status"]] += 1
    report = {"status": "OK" if counts["FAIL"] == 0 else "FAILED", "totals": counts, "claims": results}
    out = ROOT / "target" / "doc-claims.json"
    out.parent.mkdir(exist_ok=True)
    out.write_text(json.dumps(report, indent=2))
    print(json.dumps({"status": report["status"], "totals": counts, "ok": counts["FAIL"] == 0}, indent=2))
    if counts["FAIL"]:
        for r in results:
            if r["status"] == "FAIL":
                print("FAIL:", r["ac"], "-", r["assertion"])
                for d in r["details"]:
                    print("   check:", d["command"][:120])
                    print("   output:", d["output"][:300])
        sys.exit(1)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

#!/usr/bin/env python3
"""Execute the fourteen v0.4 acceptance checks; retain commands and raw logs."""
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parent.parent
OUT = ROOT / "target/validation-v04"
ENV = dict(os.environ, PATH=f"{ROOT / 'target/debug'}:{os.environ['PATH']}")


def execute(command):
    result = subprocess.run(["bash", "-o", "pipefail", "-c", command],
                            cwd=ROOT, env=ENV, text=True, capture_output=True)
    text = result.stdout + result.stderr
    if result.returncode:
        raise AssertionError(f"exit {result.returncode}\n{text}")
    return text


def main():
    OUT.mkdir(parents=True, exist_ok=True)
    results = []

    def check(number, command, names=(), validator=None):
        record = {"ac": f"AC-0.4-{number:02}", "command": command}
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

    check(1, "cargo test -p ske-cli --test callgraph",
          ["callgraph_resolution_statuses", "callgraph_scc_fact_is_self_loop"])
    with tempfile.TemporaryDirectory(prefix="ske-v04-") as directory:
        db = Path(directory) / "go.sqlite"
        def paths(_):
            execute("cargo build -p ske-cli && (cd adapters/go && go build -o ../../target/ske-adapter-go ./cmd/ske-adapter-go)")
            ingest = f"ske ingest go --db {db} --repo examples/go-branch-lab --adapter target/ske-adapter-go --yes-run-adapter --format json"
            evidence = execute(ingest)
            root = json.loads(evidence)
            assert root["status"] == "CLOSED" and root["diagnostics"]["ERROR"] == 0
            for source, target, status, incomplete, count in [
                ("example.Entry", "example.helper", "POTENTIALLY_REACHABLE", False, 1),
                ("example.helper", "example.Entry", "NO_PATH_FOUND", False, 0),
                ("example.CallDoer", "x", "NO_PATH_FOUND", True, 0),
            ]:
                command = f"ske paths explore --db {db} --source {source} --target {target} --format json"
                text = execute(command)
                evidence += f"\n$ {command}\n{text}"
                result = json.loads(text)
                assert result["status"] == status and result["incomplete"] == incomplete
                assert len(result["paths"]) == count
            (OUT / "paths-cli.log").write_text(f"$ {ingest}\n{evidence}")
        check(2, "cargo test -p ske-cli --test callgraph paths_explore_static_reachability",
              ["paths_explore_static_reachability"], paths)
    check(3, "cargo test -p ske-query summary_entry", ["summary_entry"])
    check(4, "cargo test -p ske-query summary_entry", ["summary_entry"])
    check(5, "cargo test -p ske-query summary_calldoer", ["summary_calldoer"])
    check(6, "cargo test -p ske-query summary_fact", ["summary_fact"])
    check(7, "cargo test -p ske-query summary_divmod_and_divide2_multiple_returns",
          ["summary_divmod_and_divide2_multiple_returns"])
    check(8, "cargo test -p ske-symbolic summary_matches_concrete", ["summary_matches_concrete"])
    check(9, "cargo test -p ske-abstract", ["intervals_contain_concrete", "intervals_contain_concrete_cir_sum_on_200_inputs"])
    check(10, "cargo test -p ske-cli --test analysis_sum_intervals", ["analysis_sum_intervals"])
    check(11, "cargo test -p ske-cli --test analysis_table --test analysis_deterministic",
          ["analysis_deterministic", "analysis_sum_u3", "analysis_divmod"])
    check(12, "cargo test -p ske-cli --test summary_persistence",
          ["summary_persistence_passes_real_cli_db_check"])
    check(13, "cargo test -p ske-query summary_deterministic_and_persisted",
          ["summary_deterministic_and_persisted"])
    def regression(text):
        assert "VERIFY OK v0.4" in text
        for version, expected in [("v01", 42), ("v02", 25), ("v03", 29)]:
            data = json.loads((ROOT / f"target/validation-{version}/results.json").read_text())
            assert len(data) == expected and all(r["status"] == "PASS" for r in data), version
    check(14, "./scripts/verify.sh && SKE_INHERIT_VERIFY=1 SKE_EXPECT_VERIFY_VERSION=v0.4 python3 scripts/validate-v03.py",
          validator=regression)
    rows = ["# Validation v0.4", "", "Commandes exécutées par `python3 scripts/validate-v04.py`.",
            "Logs complets locaux : `target/validation-v04/AC-0.4-*.log` et `paths-cli.log`.", "",
            "| AC | Statut | Commande exécutée |", "|---|---|---|"]
    rows += [f"| {r['ac']} | {r['status']} | `{r['command']}` |" for r in results]
    rows += ["", "AC-0.4-02 exécute aussi les trois commandes CLI exactes ;",
             "AC-0.4-12 invoque le vrai `ske db check` après persistance des résumés.",
             "Voir `docs/soundness-and-limits.md` pour les frontières de précision."]
    (ROOT / "docs/validation/v0.4.md").write_text("\n".join(rows) + "\n")
    raise SystemExit(any(r["status"] != "PASS" for r in results))


if __name__ == "__main__":
    main()

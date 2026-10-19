#!/usr/bin/env python3
"""AC-0.7-10: deterministic lexical retrieval, not general LLM quality."""
import argparse
import hashlib
import json
import os
import platform
import re
import sqlite3
import subprocess
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LAB = ROOT / "examples/go-branch-lab"
SKE = ROOT / "target/debug/ske"
ADAPTER = ROOT / "target/ske-adapter-go"
# Hand-authored independently of SKE. These are source sites, not feasible paths:
# Decide's -2 return exists lexically but is unreachable.
CORPUS = [
    dict(id="check-guards", function="Check", kind="guards",
         question="Which explicit if predicates occur in Check?",
         expected=["age < 18", "!enabled"]),
    dict(id="check-returns", function="Check", kind="returns",
         question="Which explicit return expressions occur in Check?",
         expected=["ErrUnderage", "ErrDisabled", "nil"]),
    dict(id="decide-guards", function="Decide", kind="guards",
         question="Which explicit if predicates occur in Decide?",
         expected=["x < 0", "x < -10"]),
    dict(id="decide-returns", function="Decide", kind="returns",
         question="Which explicit return expressions occur in Decide?",
         expected=["-1", "-2", "0"]),
]


def strict_json(text):
    def reject(value):
        raise ValueError(f"nonfinite JSON: {value}")
    return json.loads(text, parse_constant=reject)


def sha(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def rss_bytes(text, system):
    pattern = (r"(\d+)\s+maximum resident set size" if system == "Darwin"
               else r"Maximum resident set size \(kbytes\):\s*(\d+)")
    match = re.search(pattern, text)
    if not match:
        raise ValueError("missing peak RSS")
    return int(match[1]) * (1024 if system == "Linux" else 1)


def source_facts(text, function, kind):
    """Restricted trusted-fixture lexical parser; no execution or interpretation."""
    match = re.search(rf"^func {re.escape(function)}\([^\n]*\) [^\n]*\{{\n"
                      r"(.*?)^\}", text, re.M | re.S)
    if not match:
        raise ValueError(f"missing fixture function {function}")
    pattern = r"^\s*if (.+?) \{\s*$" if kind == "guards" else r"^\s*return (.+?)\s*$"
    return sorted(set(re.findall(pattern, match[1], re.M)))


def span_text(span):
    match = re.fullmatch(r"([^:]+):(\d+):(\d+)-(\d+):(\d+)", span or "")
    if not match or match[2] != match[4]:
        raise ValueError(f"unsupported fixture span {span}")
    path = (LAB / match[1]).resolve()
    if not path.is_relative_to(LAB.resolve()):
        raise ValueError("span outside LAB")
    line = path.read_bytes().splitlines()[int(match[2]) - 1]
    return line[int(match[3]) - 1:int(match[5]) - 1].decode().strip()


def structured_facts(q, cfg, branches):
    if q["kind"] == "guards":
        return sorted(set(span_text(d["span"]) for d in branches["source_decisions"]
                          if d["kind"] == "IF"))
    return sorted(set(span_text(i["span"]).removeprefix("return ").strip()
                      for b in cfg["blocks"] for i in b["instructions"]
                      if i["opcode"] == "RETURN"))


def ram_bytes(system, run):
    """Host RAM in bytes: sysctl hw.memsize (Darwin) or sysconf (Linux).

    `run` is the Campaign.run callable used for the Darwin sysctl query.
    """
    if system == "Darwin":
        return int(run("ram", ["sysctl", "-n", "hw.memsize"]))
    return os.sysconf("SC_PAGE_SIZE") * os.sysconf("SC_PHYS_PAGES")


def require_determinism(projections):
    """Fail closed unless the two campaign repeats are bit-identical."""
    if projections[0] != projections[1]:
        raise ValueError("inventory or analysis determinism mismatch")
    return dict(equal=True, solver_seed=0, repeats=2)


def require_integrity(check):
    """Fail closed unless `db check` reports ok."""
    if check.get("ok") is not True:
        raise ValueError("database integrity failed")


def score(answers, corpus=CORPUS):
    if set(answers) != {q["id"] for q in corpus}:
        raise ValueError("incomplete answer corpus")
    tp = predicted = expected = exact = 0
    for q in corpus:
        actual, oracle = set(answers[q["id"]]), set(q["expected"])
        tp += len(actual & oracle)
        predicted += len(actual)
        expected += len(oracle)
        exact += actual == oracle
    if not predicted or not expected:
        raise ValueError("incomplete retrieval metric")
    return dict(true_positive=tp, predicted=predicted, expected=expected,
                precision=tp / predicted, recall=tp / expected,
                exact_questions=exact, questions=len(corpus))


class Campaign:
    def __init__(self, out):
        self.out = out
        self.report = dict(benchmark="v0.7 deterministic lexical retrieval",
                           steps=[], complete=False)

    def run(self, name, argv, *, cwd=ROOT, json_output=False, memory=False):
        stdout, stderr = self.out / f"{name}.stdout", self.out / f"{name}.stderr"
        command = list(map(str, argv))
        if memory:
            command = ["/usr/bin/time",
                       "-l" if platform.system() == "Darwin" else "-v"] + command
        start = time.perf_counter()
        record = dict(name=name, argv=command, cwd=str(cwd),
                      stdout=str(stdout), stderr=str(stderr), exit=None)
        self.report["steps"].append(record)
        with stdout.open("w") as so, stderr.open("w") as se:
            proc = subprocess.run(command, cwd=cwd, stdout=so, stderr=se, timeout=240)
        record.update(exit=proc.returncode, wall_ms=(time.perf_counter() - start) * 1000)
        if proc.returncode:
            raise ValueError(f"{name}: exit {proc.returncode}; see {stderr}")
        if memory:
            record["peak_rss_bytes"] = rss_bytes(stderr.read_text(), platform.system())
        text = stdout.read_text()
        return strict_json(text) if json_output else text.strip()

    def build(self):
        """(Re)build both binaries unconditionally so measurements match HEAD."""
        self.run("build-ske", ["cargo", "build", "--locked", "-p", "ske-cli"])
        self.run("build-adapter", ["go", "build", "-o", ADAPTER, "./cmd/ske-adapter-go"],
                 cwd=ROOT / "adapters/go")

    def measure_arms(self, ske):
        """Run both retrieval arms over CORPUS; per-question costs and metrics."""
        arms = {}
        for arm in ("without_structured", "with_structured"):
            answers, costs = {}, {}
            for q in CORPUS:
                f = q["function"]
                start = time.perf_counter()
                if arm == "without_structured":
                    text = (LAB / f"{f.lower()}.go").read_text()
                    answers[q["id"]] = source_facts(text, f, q["kind"])
                else:
                    cfg = ske(f"retrieve-{q['id']}-cfg", "functions", "cfg",
                              "--function", f"example.{f}")
                    branch = ske(f"retrieve-{q['id']}-branches", "branches", "list",
                                 "--function", f"example.{f}")
                    answers[q["id"]] = structured_facts(q, cfg, branch)
                costs[q["id"]] = (time.perf_counter() - start) * 1000
            arms[arm] = dict(answers=answers, metrics=score(answers),
                             per_question_wall_ms=costs)
        return arms


def projection(db):
    with sqlite3.connect(db) as conn:
        inventory = conn.execute(
            "SELECT local_key,kind FROM source_decisions ORDER BY local_key").fetchall()
        results = conn.execute(
            "SELECT e.local_key,r.status,r.soundness,r.explanation "
            "FROM branch_analysis_results r JOIN ir_outcomes o ON o.id=r.ir_outcome_id "
            "JOIN control_edges e ON e.id=o.control_edge_id "
            "ORDER BY e.local_key,r.status,r.soundness,r.explanation").fetchall()
        if not inventory or not results or any(r[1] == "TIMEOUT" for r in results):
            raise ValueError("empty projection or TIMEOUT")
        return dict(inventory=inventory, analysis=results)


def execute(c):
    r, run = c.report, c.run
    c.build()
    system = platform.system()
    ram = ram_bytes(system, run)
    r["environment"] = dict(
        os=platform.platform(), machine=platform.machine(), ram_bytes=ram,
        cpu=run("cpu", ["sysctl", "-n", "machdep.cpu.brand_string"]) if system == "Darwin"
        else Path("/proc/cpuinfo").read_text(),
        rustc=run("rustc", ["rustc", "--version"]),
        cargo=run("cargo", ["cargo", "--version"]),
        go=run("go", ["go", "version"]), python=platform.python_version(),
        sqlite_python=sqlite3.sqlite_version,
        commit=run("commit", ["git", "rev-parse", "HEAD"]),
        dirty_tree=run("dirty", ["git", "--no-optional-locks", "status", "--porcelain"]),
        ske_sha256=sha(SKE), adapter_sha256=sha(ADAPTER))
    files = sorted(LAB.rglob("*.go"))
    r["lab"] = dict(path=str(LAB), loc=sum(len(p.read_bytes().splitlines()) for p in files),
                    source_sha256={str(p.relative_to(LAB)): sha(p) for p in files})
    r["corpus"] = CORPUS
    r["llm"] = dict(executed=False, tokens=0, monetary_cost=0,
                    quality="NOT_MEASURED: no LLM; lexical fact retrieval only")
    budget = c.out / "budget.toml"
    budget.write_text("[budget]\nsolver_seed = 0\n")
    projections = []
    for repeat in range(2):
        db = c.out / f"bench-{repeat}.sqlite"
        if db.exists():
            raise ValueError(f"refusing existing database {db}; use fresh --out")

        def ske(name, *args, memory=False):
            return run(f"{repeat}-{name}", [SKE, *args, "--db", db, "--format", "json"],
                       json_output=True, memory=memory)

        ske("ingest", "ingest", "go", "--repo", LAB, "--adapter", ADAPTER,
            "--yes-run-adapter", memory=True)
        inventory = ske("inspect", "inspect")
        if repeat == 0:
            r["inventory"] = inventory
        for function in ("Check", "Decide"):
            analysis = ske(f"analyze-{function}", "branches", "analyze", "--entry",
                           f"example.{function}", "--budget", budget)
            if analysis["run_status"] != "COMPLETED":
                raise ValueError("incomplete analysis")
        if repeat == 0:
            r["arms"] = c.measure_arms(ske)
            (c.out / "retrieval.json").write_text(json.dumps(r["arms"], indent=2) + "\n")
        require_integrity(ske("integrity", "db", "check"))
        with sqlite3.connect(db) as conn:
            r.setdefault("databases", []).append(dict(
                bytes=db.stat().st_size,
                functions=conn.execute("SELECT count(*) FROM functions WHERE is_external=0").fetchone()[0],
                source_outcomes=conn.execute("SELECT count(*) FROM source_outcomes").fetchone()[0],
                diagnostics=conn.execute("SELECT severity,count(*) FROM diagnostics GROUP BY severity").fetchall(),
                analysis=conn.execute("SELECT status,soundness,count(*) FROM branch_analysis_results GROUP BY status,soundness").fetchall(),
                solver=conn.execute("SELECT DISTINCT solver_name,solver_version,budget_json FROM analysis_runs").fetchall()))
        projections.append(projection(db))
    (c.out / "determinism.json").write_text(json.dumps(projections, indent=2) + "\n")
    r["determinism"] = require_determinism(projections)
    r["exactitude"] = "PARTIAL LAB inventory; lexical retrieval is not semantic soundness"
    r["complete"] = True


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--out", type=Path, default=ROOT / "target/bench-v07")
    parser.add_argument("--check", action="store_true", help="fail closed (also default)")
    args = parser.parse_args(argv)
    out = args.out.resolve()
    if not out.is_relative_to((ROOT / "target").resolve()):
        parser.error("--out must be under target")
    out.mkdir(parents=True, exist_ok=True)
    c = Campaign(out)
    try:
        execute(c)
    except (ValueError, KeyError, TypeError, IndexError, OSError, sqlite3.Error,
            subprocess.SubprocessError) as error:
        c.report["error"] = str(error)
    report = out / "bench-report.json"
    report.write_text(json.dumps(c.report, indent=2, allow_nan=False) + "\n")
    print(f"report: {report}; complete={c.report['complete']}")
    return 0 if c.report["complete"] else 1


if __name__ == "__main__":
    raise SystemExit(main())

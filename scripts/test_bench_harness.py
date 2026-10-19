#!/usr/bin/env python3
"""Regression guards for scripts/ske-bench.py (review-v0.7-open points 3, 4,
6, 7). Simulated command output in unit tests is harness-wiring evidence, not
acceptance evidence; the real campaign lives in validate-v07.py and in manual
runs."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "ske_bench", Path(__file__).with_name("ske-bench.py"))
b = importlib.util.module_from_spec(spec)
spec.loader.exec_module(b)

GUARDS = {"Check": ["age < 18", "!enabled"], "Decide": ["x < 0", "x < -10"]}
RETURNS = {"Check": ["ErrUnderage", "ErrDisabled", "nil"],
           "Decide": ["-1", "-2", "0"]}


def _span(path, needle):
    """Return a LAB span string locating `needle` in the real fixture file."""
    lines = (b.LAB / path).read_text().splitlines()
    for i, line in enumerate(lines):
        col = line.find(needle)
        if col >= 0:
            return f"{path}:{i + 1}:{col + 1}-{i + 1}:{col + len(needle) + 1}"
    raise AssertionError(f"{needle!r} not found in LAB/{path}")


def _out_under_target():
    (b.ROOT / "target").mkdir(parents=True, exist_ok=True)
    return Path(tempfile.mkdtemp(dir=str(b.ROOT / "target")))


def _fake_ske(fragment, *args):
    """Minimal SKE retrieval stub grounded in real LAB spans (wiring tests)."""
    function = args[args.index("--function") + 1].split(".")[1]
    path = f"{function.lower()}.go"
    if fragment.endswith("-cfg"):
        return {"blocks": [{"instructions": [
            {"opcode": "RETURN", "span": _span(path, expr)}
            for expr in RETURNS[function]]}]}
    return {"source_decisions": [
        {"kind": "IF", "span": _span(path, guard)} for guard in GUARDS[function]]}


class FailClosedTests(unittest.TestCase):
    """Review point 3: any failed step must fail the whole campaign."""

    def test_step_nonzero_exit_raises(self):
        with tempfile.TemporaryDirectory() as directory:
            c = b.Campaign(Path(directory))
            with self.assertRaises(ValueError):
                c.run("fail", ["sh", "-c", "exit 3"])

    def test_main_is_fail_closed_on_step_failure(self):
        out = _out_under_target()
        with patch.object(b.Campaign, "run",
                          side_effect=ValueError("ingest: exit 1; see stderr")):
            code = b.main(["--out", str(out)])
        report = json.loads((out / "bench-report.json").read_text())
        self.assertEqual(code, 1)
        self.assertFalse(report["complete"])
        self.assertIn("error", report)
        self.assertIn("ingest", report["error"])

    def test_projection_rejects_empty_and_timeout_projection(self):
        out = _out_under_target()
        with patch.object(b.Campaign, "run",
                          side_effect=ValueError("projection: TIMEOUT")):
            code = b.main(["--out", str(out)])
        report = json.loads((out / "bench-report.json").read_text())
        self.assertEqual(code, 1)
        self.assertFalse(report["complete"])

    def _migration_db(self, directory):
        """Build a schema-valid DB from the real normative migration."""
        import sqlite3
        db = Path(directory) / "proj.sqlite"
        con = sqlite3.connect(db)
        con.executescript((b.ROOT / "migrations/0001_initial.sql").read_text())
        con.commit()
        return con, db

    def test_projection_on_real_migration_db(self):
        """projection() parses a healthy real-schema DB and rejects TIMEOUT/empty."""
        with tempfile.TemporaryDirectory() as directory:
            con, db = self._migration_db(directory)
            # Empty DB: no inventory and no results -> fail closed.
            with self.assertRaisesRegex(ValueError, "empty projection"):
                b.projection(db)
            # Healthy inventory + POSSIBLE result -> projection returns data.
            con.execute(
                "INSERT INTO source_decisions"
                "(id, snapshot_id, unit_id, local_key, kind, source_span_id)"
                " VALUES (1, 1, 1, 's1', 'IF', 1)")
            con.execute(
                "INSERT INTO control_edges"
                "(id, local_key, source_block_id, target_block_id,"
                " terminator_instruction_id, ordinal, label, edge_kind)"
                " VALUES (1, 'e1', 1, 2, 1, 0, 'true', 'BRANCH_TRUE')")
            con.execute(
                "INSERT INTO ir_outcomes"
                "(id, decision_id, control_edge_id, ordinal, label)"
                " VALUES (100, 1, 1, 0, 't')")
            con.execute(
                "INSERT INTO branch_analysis_results"
                "(id, analysis_run_id, ir_outcome_id, status, soundness)"
                " VALUES (7, 1, 100, 'POSSIBLE', 'EXACT')")
            con.commit()
            data = b.projection(db)
            self.assertEqual(data["inventory"], [("s1", "IF")])
            self.assertEqual(data["analysis"], [("e1", "POSSIBLE", "EXACT", None)])
            # TIMEOUT result -> fail closed.
            con.execute("UPDATE branch_analysis_results SET status='TIMEOUT' WHERE id=7")
            con.commit()
            with self.assertRaisesRegex(ValueError, "TIMEOUT"):
                b.projection(db)
            con.close()


class CorpusAndArmsTests(unittest.TestCase):
    """Review point 4: independent corpus, two real arms, non-null metrics."""

    def test_corpus_is_independent_and_fixture_matches(self):
        self.assertEqual(len(b.CORPUS), 4)
        for q in b.CORPUS:
            with self.subTest(q=q["id"]):
                text = (b.LAB / f"{q['function'].lower()}.go").read_text()
                self.assertEqual(
                    b.source_facts(text, q["function"], q["kind"]),
                    sorted(q["expected"]))

    def test_score_gives_nonnull_metrics_on_exact_answers(self):
        answers = {q["id"]: sorted(q["expected"]) for q in b.CORPUS}
        metrics = b.score(answers)
        for key in ("true_positive", "predicted", "expected"):
            self.assertGreater(metrics[key], 0, key)
        self.assertEqual(metrics["exact_questions"], len(b.CORPUS))
        self.assertAlmostEqual(metrics["precision"], 1.0)
        self.assertAlmostEqual(metrics["recall"], 1.0)

    def test_score_rejects_incomplete_answers(self):
        with self.assertRaises(ValueError):
            b.score({"check-guards": []})

    def test_score_rejects_zero_metrics(self):
        with self.assertRaises(ValueError):
            b.score({q["id"]: [] for q in b.CORPUS})

    def test_both_arms_record_answers_costs_metrics(self):
        out = _out_under_target()
        arms = b.Campaign(out).measure_arms(_fake_ske)
        self.assertEqual(set(arms), {"without_structured", "with_structured"})
        for arm, data in arms.items():
            with self.subTest(arm=arm):
                self.assertEqual(set(data["answers"]),
                                 {q["id"] for q in b.CORPUS})
                self.assertEqual(set(data["per_question_wall_ms"]),
                                 {q["id"] for q in b.CORPUS})
                self.assertGreater(data["metrics"]["expected"], 0)
                self.assertGreater(data["metrics"]["predicted"], 0)


class RssAndEnvironmentTests(unittest.TestCase):
    """Review point 6: RSS units, RAM, determinism, integrity."""

    def test_linux_kbytes_converted_to_bytes(self):
        text = "Minimum resident set size (kbytes): 0\n" \
               "Maximum resident set size (kbytes): 15417\n"
        self.assertEqual(b.rss_bytes(text, "Linux"), 15417 * 1024)

    def test_darwin_bytes_left_unchanged(self):
        text = "            15417344  maximum resident set size\n"
        self.assertEqual(b.rss_bytes(text, "Darwin"), 15417344)

    def test_missing_rss_raises(self):
        with self.assertRaises(ValueError):
            b.rss_bytes("no resident set line", "Linux")

    def test_ram_bytes_darwin(self):
        self.assertEqual(b.ram_bytes("Darwin", lambda *_: "17179869184"),
                         17179869184)

    def test_ram_bytes_linux(self):
        with patch.object(b.os, "sysconf", side_effect=[4096, 1000000]):
            self.assertEqual(b.ram_bytes("Linux", None), 4096 * 1000000)

    def test_determinism_equality_required(self):
        self.assertEqual(b.require_determinism([["a"], ["a"]]),
                         {"equal": True, "solver_seed": 0, "repeats": 2})
        with self.assertRaisesRegex(ValueError, "determinism"):
            b.require_determinism([["a"], ["b"]])

    def test_integrity_ok_required(self):
        b.require_integrity({"ok": True})
        with self.assertRaisesRegex(ValueError, "integrity"):
            b.require_integrity({"ok": False})
        with self.assertRaisesRegex(ValueError, "integrity"):
            b.require_integrity({})


class BuildTests(unittest.TestCase):
    """Review point 7: binaries rebuilt every run, never gated on existence."""

    def test_build_runs_both_binaries_unconditionally(self):
        out = _out_under_target()
        c = b.Campaign(out)
        with patch.object(b.Campaign, "run") as run:
            c.build()
        names = [call.args[0] for call in run.call_args_list]
        self.assertIn("build-ske", names)
        self.assertIn("build-adapter", names)
        self.assertEqual(names[0], "build-ske")
        self.assertEqual(names[1], "build-adapter")
        args = [call.args[1] for call in run.call_args_list]
        self.assertIn(["cargo", "build", "--locked", "-p", "ske-cli"], args)
        self.assertTrue(any(a[0] == "go" and a[1] == "build" for a in args))


if __name__ == "__main__":
    unittest.main()

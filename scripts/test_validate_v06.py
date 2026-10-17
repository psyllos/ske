"""Unit tests only: no adapter builds or acceptance campaign."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "validate_v06", Path(__file__).with_name("validate-v06.py"))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)


class HarnessTests(unittest.TestCase):
    def test_zero_filter_rejected(self):
        with self.assertRaises(AssertionError):
            v.require_tests("test result: ok. 0 passed; 0 failed; 0 ignored;", "lang_types")

    def test_other_filter_rejected(self):
        with self.assertRaises(AssertionError):
            v.require_tests("test other ... ok\ntest result: ok. 1 passed;", "lang_types")

    def test_real_test_accepted(self):
        v.require_tests("test lang_types ... ok\ntest result: ok. 1 passed; 0 failed; 0 ignored;", "lang_types")

    def test_ignored_rejected(self):
        with self.assertRaises(AssertionError):
            v.require_tests("test lang_types ... ok\ntest result: ok. 1 passed; 1 ignored;", "lang_types")

    def test_missing_and_failed_prerequisites_rejected(self):
        for rows in ([], [{"number": 1, "status": "FAIL"}],
                     [{"number": 1, "status": "NOT_RUN"}]):
            with self.subTest(rows=rows), self.assertRaises(AssertionError):
                v.require_passes(rows, [1])

    def run_subset(self, only, effect):
        with tempfile.TemporaryDirectory() as tmp:
            out = Path(tmp)
            with patch.object(v, "OUT", out), patch.object(v, "execute", side_effect=effect) as execute:
                code = v.main(["--only", only])
                rows = json.loads((out / "results.json").read_text())
                return code, rows, execute.call_count

    def test_only_preserves_not_run(self):
        code, rows, calls = self.run_subset("1", lambda _: "")
        self.assertEqual(code, 1)
        self.assertEqual(calls, 2)
        self.assertEqual(rows[0]["status"], "PASS")
        self.assertTrue(all(r["status"] == "NOT_RUN" for r in rows[1:]))

    def test_failed_build_blocks_test(self):
        def fail(_):
            raise AssertionError("build failed")
        _, rows, calls = self.run_subset("1,3", fail)
        self.assertEqual(calls, 1)
        self.assertEqual(rows[2]["status"], "FAIL")
        self.assertIn("prerequisites", rows[2]["error"])

    def test_freeze_and_regression_need_all_prerequisites(self):
        _, rows, calls = self.run_subset("12,13", lambda _: "")
        self.assertEqual(calls, 0)
        self.assertEqual(rows[11]["status"], "FAIL")
        self.assertEqual(rows[12]["status"], "FAIL")

    def test_inherited_environment_removed(self):
        self.assertNotIn("SKE_INHERIT_VERIFY", v.runner.ENV)
        self.assertEqual(v.runner.ENV["SKE_EXPECT_VERIFY_VERSION"], "v0.6")

    def test_regression_requires_exact_unique_ac_ids(self):
        with tempfile.TemporaryDirectory() as tmp, patch.object(v, "ROOT", Path(tmp)):
            for version, count in v.COUNTS.items():
                out = Path(tmp) / f"target/validation-{version}"
                out.mkdir(parents=True)
                rows = [{"ac": f"AC-0.{int(version[1:])}-{n:02}", "status": "PASS"}
                        for n in range(1, count + 1)]
                (out / "results.json").write_text(json.dumps(rows))
            v.validate_regressions()
            path = Path(tmp) / "target/validation-v05/results.json"
            rows[-1] = rows[0]
            path.write_text(json.dumps(rows))
            with self.assertRaises(AssertionError):
                v.validate_regressions()


if __name__ == "__main__":
    unittest.main()

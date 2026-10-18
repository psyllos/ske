"""Harness unit tests: simulated command output is not acceptance evidence."""
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

spec = importlib.util.spec_from_file_location(
    "validate_v07", Path(__file__).with_name("validate-v07.py"))
v = importlib.util.module_from_spec(spec)
spec.loader.exec_module(v)


class AcceptanceCoverageTests(unittest.TestCase):
    def test_successful_filter_marks_pass_only_with_named_test(self):
        output = ("test dto_example ... ok\n"
                  "test result: ok. 1 passed; 0 failed; 0 ignored;\n")
        with tempfile.TemporaryDirectory() as directory:
            out = Path(directory)
            with patch.object(v, "OUT", out), patch.object(
                    v, "execute", side_effect=[output, ""]):
                code = v.main(["--only", "1"])
            rows = json.loads((out / "results.json").read_text())
        self.assertEqual(code, 1)  # selected subset: others NOT_RUN keep exit 1
        self.assertEqual(rows[0]["status"], "PASS")
        self.assertEqual(rows[0]["checks_succeeded"], True)
        self.assertTrue(all(row["status"] == "NOT_RUN" for row in rows[1:]))

    def test_matching_output_without_named_test_is_rejected(self):
        with self.assertRaises(AssertionError):
            v.require_tests("test result: ok. 5 passed; 0 failed;", "dto_")

    def test_partial_criterion_blocks_acceptance_regression(self):
        with self.assertRaises(AssertionError):
            v.require_passes([{"number": 1, "status": "PARTIAL"}], [1])

    def test_empty_filter_is_not_evidence(self):
        with self.assertRaises(AssertionError):
            v.require_tests("test result: ok. 0 passed; 0 failed;", "dto_")

    def test_regression_rebuilds_v06_not_only_v05(self):
        self.assertEqual(
            v.COMMANDS[11],
            ["SKE_EXPECT_VERIFY_VERSION=v0.7 python3 scripts/validate-v06.py"],
        )


if __name__ == "__main__":
    unittest.main()

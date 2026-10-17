"""Fast regression-gate checks; do not launch the acceptance workflow."""
import importlib.util
from pathlib import Path
import unittest

spec = importlib.util.spec_from_file_location(
    "validate_v05", Path(__file__).with_name("validate-v05.py"))
validator = importlib.util.module_from_spec(spec)
spec.loader.exec_module(validator)


class GateTests(unittest.TestCase):
    def test_failed_prerequisite_blocks_final_campaign(self):
        with self.assertRaisesRegex(AssertionError, "AC-0.5-09"):
            validator.require_regression_ready([
                {"ac": "AC-0.5-09", "status": "FAIL"}])

    def test_green_prerequisites_allow_final_campaign(self):
        validator.require_regression_ready([
            {"ac": "AC-0.5-09", "status": "PASS"}])

    def test_explicit_only_preserves_not_run_semantics(self):
        validator.require_regression_ready([
            {"ac": "AC-0.5-09", "status": "NOT_RUN"}])


if __name__ == "__main__":
    unittest.main()

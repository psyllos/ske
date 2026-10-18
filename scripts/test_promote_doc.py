"""Unit test for the SKE_PROMOTE_DOC gate (v0.3-v0.5 validators).

A campaign must write its artefact report under target/ and must NOT modify
docs/validation/v0.X.md unless SKE_PROMOTE_DOC=1 is set. This keeps
hand-maintained "validée" framing intact during nested regression chains."""
import importlib.util
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch


def load(name):
    spec = importlib.util.spec_from_file_location(
        name.replace("-", "_"), Path(__file__).with_name(f"{name}.py"))
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class PromoteDocGateTest(unittest.TestCase):
    def run_validator(self, name, doc, promote):
        module = load(name)
        dir_ = tempfile.TemporaryDirectory()
        self.addCleanup(dir_.cleanup)
        root = Path(dir_.name)
        (root / "docs" / "validation").mkdir(parents=True)
        old = os.environ.get("SKE_PROMOTE_DOC")
        if promote:
            os.environ["SKE_PROMOTE_DOC"] = "1"
        else:
            os.environ.pop("SKE_PROMOTE_DOC", None)
        try:
            patchers = [patch.object(module, "ROOT", root),
                        patch.object(module, "execute", side_effect=lambda _: "")]
            if hasattr(module, "OUT"):
                patchers.append(patch.object(module, "OUT", root / "target" / "validation-vXX"))
            for patcher in patchers:
                patcher.start()
            try:
                argv, sys.argv = sys.argv, ["validate"]
                try:
                    module.main()
                except SystemExit:
                    pass
                finally:
                    sys.argv = argv
            finally:
                for patcher in reversed(patchers):
                    patcher.stop()
        finally:
            if old is None:
                os.environ.pop("SKE_PROMOTE_DOC", None)
            else:
                os.environ["SKE_PROMOTE_DOC"] = old
        version = name.rsplit("-", 1)[1]
        out = root / "target" / ("validation-vXX" if hasattr(module, "OUT") else f"validation-{version}")
        return root, out

    def test_default_writes_report_only(self):
        for name, doc in [("validate-v03", "docs/validation/v0.3.md"),
                          ("validate-v04", "docs/validation/v0.4.md"),
                          ("validate-v05", "docs/validation/v0.5.md")]:
            with self.subTest(name=name):
                root, out = self.run_validator(name, doc, promote=False)
                self.assertTrue((out / "report.md").is_file(),
                                f"{name}: artefact report.md missing")
                self.assertFalse((root / doc).exists(),
                                 f"{name}: tracked doc written without promotion")

    def test_promote_writes_tracked_doc(self):
        for name, doc in [("validate-v03", "docs/validation/v0.3.md"),
                          ("validate-v04", "docs/validation/v0.4.md"),
                          ("validate-v05", "docs/validation/v0.5.md")]:
            with self.subTest(name=name):
                root, out = self.run_validator(name, doc, promote=True)
                self.assertTrue((out / "report.md").is_file())
                self.assertTrue((root / doc).is_file(),
                                f"{name}: tracked doc not written with SKE_PROMOTE_DOC=1")


if __name__ == "__main__":
    unittest.main()

import copy
import io
import json
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout
from pathlib import Path

import generate_formula as generator

HERE = Path(__file__).resolve().parent
FIXTURE = HERE / "fixtures" / "release-v0.1.17.json"


def load_release():
    return json.loads(FIXTURE.read_text(encoding="utf-8"))


class RenderTest(unittest.TestCase):
    def setUp(self):
        self.release = load_release()

    def test_renders_release_assets_and_install_name(self):
        version, text = generator.render_formula(self.release, "v0.1.17")
        self.assertEqual(version, (0, 1, 17))
        self.assertIn('version "0.1.17"', text)
        self.assertIn('license "Apache-2.0"', text)
        self.assertIn(
            "https://github.com/cloudiful/git-ai-commit/releases/download/"
            "v0.1.17/git-ai-commit-macos-arm64",
            text,
        )
        self.assertIn(
            "bb23e5f1001c264552a734d2761e6742c6f5eec5a9536d5bff5827a9410a3c12",
            text,
        )
        self.assertIn(
            "ebfb27e1b904e6366ba92c00a318f23e077fd2bd849e3c9e3dcc4eb2a848e3f4",
            text,
        )
        self.assertIn(
            "0169c01eaae1f306ca7d52c20e358853d99d9c8208f70fce6475696aaa248f63",
            text,
        )
        self.assertIn("on_macos do", text)
        self.assertIn("on_arm do", text)
        self.assertIn("on_linux do", text)
        self.assertIn("on_intel do", text)
        self.assertIn(
            'bin.install Dir["git-ai-commit-*"].first => "git-ai-commit"', text
        )
        self.assertIn(
            'shell_output("#{bin}/git-ai-commit --version")', text
        )
        self.assertNotIn("x86_64-apple-darwin", text)

    def test_rejects_draft_release(self):
        self.release["draft"] = True
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_prerelease(self):
        self.release["prerelease"] = True
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_tag_mismatch(self):
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release, "v0.1.16")

    def test_rejects_non_stable_tag(self):
        self.release["tag_name"] = "v0.1.17-rc.1"
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_shell_injection_tag(self):
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release, "v0.1.17; rm -rf /")

    def test_rejects_missing_asset(self):
        self.release["assets"] = [
            asset
            for asset in self.release["assets"]
            if asset["name"] != "git-ai-commit-linux-arm64"
        ]
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_duplicate_asset(self):
        self.release["assets"].append(copy.deepcopy(self.release["assets"][0]))
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_missing_digest(self):
        asset = self.release["assets"][0]
        asset.pop("digest")
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_malformed_digest(self):
        self.release["assets"][0]["digest"] = "sha256:abc"
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_unuploaded_asset(self):
        self.release["assets"][0]["state"] = "starter"
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)

    def test_rejects_unexpected_download_url(self):
        self.release["assets"][0][
            "browser_download_url"
        ] = "https://example.com/git-ai-commit-linux-amd64"
        with self.assertRaises(generator.FormulaError):
            generator.render_formula(self.release)


class TapWriteTest(unittest.TestCase):
    def setUp(self):
        self.release = load_release()
        self.temporary = tempfile.TemporaryDirectory()
        self.tap = Path(self.temporary.name)
        (self.tap / "Formula").mkdir()

    def tearDown(self):
        self.temporary.cleanup()

    def run_cli(self, *arguments):
        stdout = io.StringIO()
        stderr = io.StringIO()
        with redirect_stdout(stdout), redirect_stderr(stderr):
            code = generator.main(
                [
                    "--tag",
                    "v0.1.17",
                    "--release-json",
                    str(FIXTURE),
                    *arguments,
                ]
            )
        return code, stdout.getvalue(), stderr.getvalue()

    def formula_text(self) -> str:
        return (self.tap / generator.FORMULA_PATH).read_text(encoding="utf-8")

    def test_writes_once_and_identical_rerun_is_noop(self):
        code, stdout, _ = self.run_cli("--tap", str(self.tap))
        self.assertEqual(code, 0)
        self.assertIn("wrote", stdout)
        first = self.formula_text()

        code, stdout, _ = self.run_cli("--tap", str(self.tap))
        self.assertEqual(code, 0)
        self.assertIn("unchanged", stdout)
        self.assertEqual(self.formula_text(), first)

    def test_dry_run_prints_formula_without_writing(self):
        code, stdout, _ = self.run_cli("--dry-run")
        self.assertEqual(code, 0)
        self.assertIn('version "0.1.17"', stdout)
        self.assertFalse((self.tap / generator.FORMULA_PATH).exists())

    def test_rejects_downgrade(self):
        existing = (
            'class GitAiCommit < Formula\n  version "0.2.0"\nend\n'
        )
        (self.tap / generator.FORMULA_PATH).write_text(existing, encoding="utf-8")

        code, _, stderr = self.run_cli("--tap", str(self.tap))
        self.assertEqual(code, 1)
        self.assertIn("downgrade", stderr)
        self.assertEqual(self.formula_text(), existing)

    def test_rejects_same_version_with_different_content(self):
        existing = (
            'class GitAiCommit < Formula\n  version "0.1.17"\nend\n'
        )
        (self.tap / generator.FORMULA_PATH).write_text(existing, encoding="utf-8")

        code, _, stderr = self.run_cli("--tap", str(self.tap))
        self.assertEqual(code, 1)
        self.assertIn("different content", stderr)
        self.assertEqual(self.formula_text(), existing)

    def test_rejects_missing_formula_directory(self):
        empty = Path(self.temporary.name) / "empty"
        empty.mkdir()
        code, _, stderr = self.run_cli("--tap", str(empty))
        self.assertEqual(code, 1)
        self.assertIn("not a tap checkout", stderr)


if __name__ == "__main__":
    unittest.main()

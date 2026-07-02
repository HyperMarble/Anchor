from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from execroot_mode import apply_execroot_patch, prepare_execroot


def init_git(path: Path) -> None:
    subprocess.run(["git", "init", "-q"], cwd=path, check=True)
    subprocess.run(["git", "config", "user.email", "test@example.invalid"], cwd=path, check=True)
    subprocess.run(["git", "config", "user.name", "Execroot Test"], cwd=path, check=True)
    subprocess.run(["git", "add", "-A"], cwd=path, check=True)
    subprocess.run(["git", "commit", "-q", "-m", "base"], cwd=path, check=True)


class ExecrootModeTests(unittest.TestCase):
    def test_execroot_patch_applies_without_touching_skipped_outputs(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            repo.mkdir()
            (repo / "src.txt").write_text("old\n")
            (repo / ".anchor").mkdir()
            (repo / ".anchor" / "events.jsonl").write_text("internal\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            self.assertFalse((execroot / ".anchor").exists())
            (execroot / "src.txt").write_text("new\n")
            (execroot / "created.txt").write_text("created\n")
            (execroot / "__pycache__").mkdir()
            (execroot / "__pycache__" / "x.pyc").write_bytes(b"cache")

            result = apply_execroot_patch(execroot, repo, logs)

            self.assertEqual(result["apply_exit_code"], 0)
            self.assertEqual((repo / "src.txt").read_text(), "new\n")
            self.assertEqual((repo / "created.txt").read_text(), "created\n")
            self.assertFalse((repo / "__pycache__").exists())
            self.assertIn("src.txt", result["changed_paths"])
            self.assertIn("created.txt", result["changed_paths"])
            patch = Path(result["patch_path"]).read_text()
            self.assertIn("+new", patch)
            self.assertIn("+created", patch)

    def test_prepare_execroot_writes_spec_ranker_action_packet(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            repo.mkdir()
            (repo / "app.py").write_text("def route():\n    return 1\n")
            init_git(repo)
            fake_anchor = root / "anchor"
            fake_anchor.write_text(
                "#!/usr/bin/env python3\n"
                "raise SystemExit('prepare_execroot must not pre-query raw task text')\n"
            )
            fake_anchor.chmod(0o755)

            execroot = prepare_execroot(
                repo,
                run_dir,
                instruction="fix route behavior",
                anchor_bin=fake_anchor,
            )

            self.assertTrue((execroot / "ANCHOR_ACTION.md").exists())
            self.assertTrue((execroot / ".anchor" / "action" / "spec-query.md").exists())
            self.assertFalse((execroot / ".anchor" / "action" / "query.json").exists())
            self.assertIn("fix route behavior", (execroot / "ANCHOR_ACTION.md").read_text())
            self.assertIn("ExecutionSpec", (execroot / "ANCHOR_ACTION.md").read_text())


if __name__ == "__main__":
    unittest.main()

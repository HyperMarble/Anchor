from __future__ import annotations

import json
import shutil
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


def write_semantic_owner(execroot: Path, path: str, name: str = "value") -> Path:
    owner_dir = execroot / ".anchor" / "semantic" / "current" / "by-task" / "owners"
    owner_dir.mkdir(parents=True, exist_ok=True)
    doc = owner_dir / f"01_{name}.md"
    doc.write_text(
        "# Owner Chunk\n\n"
        f"handle: `chunk:{path}#{name}@1-2`\n"
        f"path: `{path}`\n"
        f"symbol: `{name}`\n"
        "source_hash: `test-hash`\n"
        "\n```text\n"
        "1: def value():\n"
        "2:     return 1\n"
        "```\n"
    )
    return doc


class ExecrootGateTests(unittest.TestCase):
    def test_execroot_rejects_source_patch_without_anchor_read(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(execroot, repo, logs)

            self.assertFalse(result["accepted"])
            self.assertEqual(result["apply_exit_code"], None)
            self.assertEqual((repo / "app.py").read_text(), "def value():\n    return 1\n")
            self.assertIn("missing_semantic_contract", result["gate"]["reasons"])
            self.assertTrue((logs / "execroot-rejected.json").exists())

    def test_execroot_accepts_source_patch_with_contract_and_event_read(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            write_semantic_owner(execroot, "app.py")
            events = execroot / ".anchor" / "events"
            events.mkdir(parents=True)
            rows = [
                {"event_type": "semantic.contract", "status": "ok"},
                {"event_type": "context.read", "status": "ok", "path": "app.py"},
            ]
            (events / "events.jsonl").write_text("\n".join(json.dumps(row) for row in rows))
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(execroot, repo, logs)

            self.assertTrue(result["accepted"])
            self.assertEqual(result["apply_exit_code"], 0)
            self.assertEqual((repo / "app.py").read_text(), "def value():\n    return 2\n")

    def test_execroot_rejects_when_agent_deleted_semantic_contract(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            agent_log = root / "codex.jsonl"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            write_semantic_owner(execroot, "app.py")
            shutil.rmtree(execroot / ".anchor", ignore_errors=True)
            rows = [
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "/tmp/anchor query --limit 12 value",
                        "exit_code": 0,
                    },
                },
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "/tmp/anchor read chunk:app.py#value@1-2",
                        "exit_code": 0,
                    },
                },
            ]
            agent_log.write_text("\n".join(json.dumps(row) for row in rows))
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(
                execroot,
                repo,
                logs,
                agent_log_path=agent_log,
                anchor_bin=Path("/tmp/anchor"),
            )

            self.assertFalse(result["accepted"])
            self.assertEqual(result["apply_exit_code"], None)
            self.assertEqual((repo / "app.py").read_text(), "def value():\n    return 1\n")
            self.assertIn("missing_semantic_contract", result["gate"]["reasons"])

    def test_execroot_accepts_semantic_workspace_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            agent_log = root / "codex.jsonl"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            owner_doc = write_semantic_owner(execroot, "app.py")
            rows = [
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "/tmp/anchor semantic value --limit 8 --context-limit 4",
                        "exit_code": 0,
                    },
                },
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": f"cat {owner_doc.relative_to(execroot)}",
                        "exit_code": 0,
                    },
                },
            ]
            agent_log.write_text("\n".join(json.dumps(row) for row in rows))
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(
                execroot,
                repo,
                logs,
                agent_log_path=agent_log,
                anchor_bin=Path("/tmp/anchor"),
            )

            self.assertTrue(result["accepted"])
            self.assertEqual(result["apply_exit_code"], 0)
            self.assertEqual((repo / "app.py").read_text(), "def value():\n    return 2\n")

    def test_execroot_rejects_changed_file_outside_semantic_contract(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            agent_log = root / "codex.jsonl"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            (repo / "other.py").write_text("def other():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            owner_doc = write_semantic_owner(execroot, "other.py", "other")
            rows = [
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "/tmp/anchor semantic other --limit 8 --context-limit 4",
                        "exit_code": 0,
                    },
                },
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": f"cat {owner_doc.relative_to(execroot)}",
                        "exit_code": 0,
                    },
                },
            ]
            agent_log.write_text("\n".join(json.dumps(row) for row in rows))
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(
                execroot,
                repo,
                logs,
                agent_log_path=agent_log,
                anchor_bin=Path("/tmp/anchor"),
            )

            self.assertFalse(result["accepted"])
            self.assertIn(
                "changed_file_outside_semantic_contract", result["gate"]["reasons"]
            )

    def test_execroot_rejects_index_only_semantic_read(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            repo = root / "repo"
            run_dir = root / "run"
            logs = root / "logs"
            agent_log = root / "codex.jsonl"
            repo.mkdir()
            (repo / "app.py").write_text("def value():\n    return 1\n")
            init_git(repo)

            execroot = prepare_execroot(repo, run_dir)
            write_semantic_owner(execroot, "app.py")
            index = execroot / ".anchor" / "semantic" / "current" / "index.md"
            index.write_text("# Anchor Semantic Workspace\n")
            rows = [
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "/tmp/anchor semantic value --limit 8 --context-limit 4",
                        "exit_code": 0,
                    },
                },
                {
                    "type": "item.completed",
                    "item": {
                        "type": "command_execution",
                        "command": "cat .anchor/semantic/current/index.md",
                        "exit_code": 0,
                    },
                },
            ]
            agent_log.write_text("\n".join(json.dumps(row) for row in rows))
            (execroot / "app.py").write_text("def value():\n    return 2\n")
            result = apply_execroot_patch(
                execroot,
                repo,
                logs,
                agent_log_path=agent_log,
                anchor_bin=Path("/tmp/anchor"),
            )

            self.assertFalse(result["accepted"])
            self.assertIn("changed_file_without_semantic_read", result["gate"]["reasons"])


if __name__ == "__main__":
    unittest.main()

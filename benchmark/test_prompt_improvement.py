from __future__ import annotations

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from prompt_improvement import build_profile, improve_prompt, matching_project_targets


class PromptImprovementTests(unittest.TestCase):
    def test_build_profile_collects_repo_instruction_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text("[package]\nname = 'fixture'\n")
            (root / "README.md").write_text("# Fixture\n")
            (root / "AGENTS.md").write_text("Use repo rules first.\n")
            (root / ".continue" / "rules").mkdir(parents=True)
            (root / ".continue" / "rules" / "python.md").write_text("Prefer pytest.\n")
            (root / ".cursor" / "rules").mkdir(parents=True)
            (root / ".cursor" / "rules" / "editing.mdc").write_text("Keep edits scoped.\n")

            profile = build_profile(root)

            self.assertEqual(
                profile.instruction_files,
                [
                    ".continue/rules/python.md",
                    ".cursor/rules/editing.mdc",
                    "AGENTS.md",
                ],
            )

    def test_prompt_rules_request_surfaces_instruction_files_in_brief_and_targets(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text("[package]\nname = 'fixture'\n")
            (root / "README.md").write_text("# Fixture\n")
            (root / "benchmark" / "claude").mkdir(parents=True)
            (root / "benchmark" / "claude" / "CLAUDE.anchor.md").write_text(
                "Use Anchor first.\n"
            )

            profile = build_profile(root)
            case = {
                "id": "rules",
                "human_prompt": "the prompt rules are too bossy, fix the claude handoff",
            }

            repaired = improve_prompt(case, profile)
            targets = matching_project_targets(case["human_prompt"], profile)
            target_paths = [item.path for item in targets]

            self.assertIn("benchmark/claude/CLAUDE.anchor.md", repaired)
            self.assertIn("Honor the repo-local agent instructions", repaired)
            self.assertIn("benchmark/claude/CLAUDE.anchor.md", target_paths)


if __name__ == "__main__":
    unittest.main()

import json
import tempfile
import unittest
from pathlib import Path

from benchmark import prompt_improvement


class PromptImprovementTests(unittest.TestCase):
    def write_product_memory(self, root: Path, payload: dict) -> None:
        anchor_dir = root / ".anchor"
        anchor_dir.mkdir(parents=True, exist_ok=True)
        (anchor_dir / "product_memory.json").write_text(
            json.dumps(payload), encoding="utf-8"
        )

    def test_load_product_memory_reads_instruction_files(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                '[package]\nname = "fixture"\nversion = "0.1.0"\n', encoding="utf-8"
            )
            (root / "AGENTS.md").write_text(
                "# Repo rules\nInspect tests first.\n", encoding="utf-8"
            )
            self.write_product_memory(
                root,
                {
                    "schema": "anchor.product_memory.v1",
                    "instruction_files": [
                        {
                            "path": "AGENTS.md",
                            "kind": "agent_rules",
                            "note": "Repo-local agent instructions for coding sessions.",
                            "source_hash": "a" * 64,
                        }
                    ],
                },
            )

            evidence = prompt_improvement.load_product_memory(root)

            self.assertEqual(len(evidence), 1)
            self.assertEqual(evidence[0].source, "AGENTS.md")
            self.assertEqual(evidence[0].kind, "agent_rules")
            self.assertIn("instructions", evidence[0].detail)

    def test_improve_prompt_surfaces_cached_instruction_memory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                '[package]\nname = "fixture"\nversion = "0.1.0"\n', encoding="utf-8"
            )
            (root / "README.md").write_text("Anchor fixture repo.\n", encoding="utf-8")
            (root / "docs").mkdir()
            (root / "docs" / "install.sh").write_text("#!/usr/bin/env bash\n", encoding="utf-8")
            (root / "AGENTS.md").write_text(
                "# Repo rules\nInspect tests first.\n", encoding="utf-8"
            )
            self.write_product_memory(
                root,
                {
                    "schema": "anchor.product_memory.v1",
                    "facts": [
                        {
                            "source": "README.md",
                            "fact": "Anchor is a repo-local execution harness for coding agents.",
                        }
                    ],
                    "instruction_files": [
                        {
                            "path": "AGENTS.md",
                            "kind": "agent_rules",
                            "note": "Repo-local agent instructions for coding sessions.",
                            "source_hash": "b" * 64,
                        }
                    ],
                },
            )

            profile = prompt_improvement.build_profile(root)
            brief = prompt_improvement.improve_prompt(
                {"id": "case1", "human_prompt": "docs say the prompt rules are too bossy"},
                profile,
            )

            self.assertIn("Product memory evidence:", brief)
            self.assertIn("AGENTS.md", brief)
            self.assertIn("Repo-local agent instructions are cached in AGENTS.md", brief)
            self.assertIn("attached 2 cached product-memory evidence item(s)", brief)


if __name__ == "__main__":
    unittest.main()

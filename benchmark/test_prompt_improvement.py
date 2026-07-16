from __future__ import annotations

import json
import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))

from prompt_improvement import build_profile, improve_prompt


class PromptImprovementTests(unittest.TestCase):
    def test_build_profile_reads_cached_product_memory(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".anchor").mkdir()
            (root / ".anchor" / "product_memory.json").write_text(
                json.dumps(
                    {
                        "schema": "anchor.product_memory.v1",
                        "source_hash": "abc123",
                        "facts": [
                            {
                                "source": "README.md",
                                "fact": "Anchor Prompt Repair grounds coding tasks in repository facts before an agent edits code.",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            profile = build_profile(root)

            self.assertEqual(1, len(profile.product_memory))
            self.assertEqual("README.md", profile.product_memory[0].source)
            self.assertIn("grounds coding tasks", profile.product_memory[0].fact)

    def test_build_profile_extracts_product_memory_without_cache(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "README.md").write_text(
                "# Anchor\n\n"
                "Anchor Prompt Repair rewrites messy coding requests into repository-aware task briefs before agents start editing code.\n",
                encoding="utf-8",
            )
            (root / "docs").mkdir()
            (root / "docs" / "prompt-repair.md").write_text(
                "# Prompt Repair\n\n"
                "- The default repair path stays deterministic and local to the repository context.\n",
                encoding="utf-8",
            )
            (root / "Cargo.toml").write_text(
                '[package]\nname = "anchor"\ndescription = "Repository-aware CLI for prompt repair and agent context."\n',
                encoding="utf-8",
            )

            profile = build_profile(root)
            evidence = [(item.source, item.fact) for item in profile.product_memory]

            self.assertIn(
                (
                    "README.md",
                    "Anchor Prompt Repair rewrites messy coding requests into repository-aware task briefs before agents start editing code",
                ),
                evidence,
            )
            self.assertTrue(any(source == "Cargo.toml" for source, _ in evidence))

    def test_improve_prompt_cites_product_memory_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "Cargo.toml").write_text(
                '[package]\nname = "anchor"\nversion = "0.1.0"\nedition = "2021"\n',
                encoding="utf-8",
            )
            (root / "README.md").write_text(
                "# Anchor\n\n"
                "Anchor keeps coding agents grounded in repository facts and verified checks.\n",
                encoding="utf-8",
            )

            profile = build_profile(root)
            repaired = improve_prompt(
                {"id": "repair-docs", "human_prompt": "tighten the agent handoff brief"},
                profile,
            )

            self.assertIn("Product memory evidence:", repaired)
            self.assertIn("source: README.md", repaired)
            self.assertIn("repository facts and verified checks", repaired)


if __name__ == "__main__":
    unittest.main()

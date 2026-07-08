import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("prompt_improvement.py")
SPEC = importlib.util.spec_from_file_location("prompt_improvement", MODULE_PATH)
prompt_improvement = importlib.util.module_from_spec(SPEC)
assert SPEC.loader is not None
sys.modules[SPEC.name] = prompt_improvement
SPEC.loader.exec_module(prompt_improvement)


class PromptImprovementCallIndexTests(unittest.TestCase):
    def create_repo(self) -> Path:
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        root = Path(tmp.name)
        (root / "Cargo.toml").write_text(
            "[package]\nname = 'demo'\nversion = '0.1.0'\n",
            encoding="utf-8",
        )
        (root / "src").mkdir()
        (root / "src" / "auth.rs").write_text("pub fn login() {}\n", encoding="utf-8")
        (root / "src" / "session.rs").write_text(
            "pub fn establish_session() {}\n",
            encoding="utf-8",
        )
        (root / ".anchor" / "index").mkdir(parents=True)
        (root / ".anchor" / "index" / "symbols.json").write_text(
            json.dumps(
                {
                    "symbols": [
                        {"name": "login", "path": "src/auth.rs"},
                        {"name": "establish_session", "path": "src/session.rs"},
                    ]
                }
            ),
            encoding="utf-8",
        )
        (root / ".anchor" / "index" / "calls.json").write_text(
            json.dumps({"calls": {"login": ["establish_session"]}}),
            encoding="utf-8",
        )
        return root

    def test_build_profile_loads_call_index(self) -> None:
        profile = prompt_improvement.build_profile(self.create_repo())

        self.assertEqual(profile.call_index, {"login": ["establish_session"]})
        self.assertEqual(profile.symbol_paths["login"], ["src/auth.rs"])

    def test_matching_targets_adds_call_neighbor_files(self) -> None:
        profile = prompt_improvement.build_profile(self.create_repo())

        targets = prompt_improvement.matching_project_targets("Fix the login flow", profile)
        target_paths = {target.path for target in targets}

        self.assertIn("src/auth.rs", target_paths)
        self.assertIn("src/session.rs", target_paths)
        self.assertTrue(
            any(
                "calls symbol" in target.reason
                for target in targets
                if target.path == "src/session.rs"
            )
        )


if __name__ == "__main__":
    unittest.main()

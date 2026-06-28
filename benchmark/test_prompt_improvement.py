import importlib.util
import json
import sys
import tempfile
import unittest
from pathlib import Path


def load_prompt_improvement_module():
    module_path = Path(__file__).with_name("prompt_improvement.py")
    spec = importlib.util.spec_from_file_location("prompt_improvement", module_path)
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


prompt_improvement = load_prompt_improvement_module()


class PromptImprovementProfileTests(unittest.TestCase):
    def test_build_profile_detects_frameworks_from_package_json(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / "package.json").write_text(
                json.dumps(
                    {
                        "dependencies": {"react": "18.0.0", "express": "5.0.0"},
                        "devDependencies": {"jest": "29.0.0"},
                    }
                ),
                encoding="utf-8",
            )

            profile = prompt_improvement.build_profile(root)
            warnings = prompt_improvement.incorrect_assumptions(
                "fix the react component and jest coverage in the express app",
                profile,
            )

            self.assertEqual([], warnings)

    def test_cached_profile_reuses_framework_signals(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / ".anchor").mkdir()
            (root / ".anchor" / "project_profile.json").write_text(
                json.dumps(
                    {
                        "languages": ["TypeScript"],
                        "top_dirs": ["src"],
                        "key_files": ["README.md"],
                        "indexed_files": ["src/app.tsx"],
                        "manifests": ["package.json"],
                        "test_commands": ["npm test"],
                        "frameworks_present": ["react"],
                        "frameworks_absent": ["express", "jest", "next"],
                    }
                ),
                encoding="utf-8",
            )

            profile = prompt_improvement.build_profile(root)
            warnings = prompt_improvement.incorrect_assumptions(
                "fix the react component but do not migrate to next",
                profile,
            )

            self.assertIn("react", profile.frameworks_present)
            self.assertNotIn("react: No React app was detected.", warnings)
            self.assertIn("next: No Next.js app was detected.", warnings)


if __name__ == "__main__":
    unittest.main()

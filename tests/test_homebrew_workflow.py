# SPDX-License-Identifier: AGPL-3.0-or-later
from pathlib import Path
import unittest
import yaml

ROOT = Path(__file__).resolve().parents[1]


def load(name):
    path = ROOT / ".github/workflows" / name
    return yaml.load(path.read_text(), Loader=yaml.BaseLoader) if path.exists() else {}


class HomebrewWorkflowTests(unittest.TestCase):
    def test_published_releases_reusable_call_and_manual_retries(self):
        triggers = load("homebrew.yml").get("on", {})
        self.assertTrue({"release", "workflow_call", "workflow_dispatch"} <= set(triggers))
        self.assertIn("published", triggers["release"]["types"])
        self.assertEqual(triggers["workflow_call"]["secrets"]["HOMEBREW_TAP_SSH_KEY"]["required"], "true")

    def test_release_publisher_explicitly_calls_homebrew_after_uploads(self):
        caller = load("release.yml").get("jobs", {}).get("homebrew", {})
        self.assertEqual(caller.get("needs"), "release")
        self.assertEqual(caller.get("uses"), "./.github/workflows/homebrew.yml")
        self.assertIn("HOMEBREW_TAP_SSH_KEY", caller.get("secrets", {}))
        self.assertIn("tag", caller.get("with", {}))

    def test_publication_serializes_and_uses_read_only_application_token(self):
        workflow = load("homebrew.yml")
        self.assertEqual(workflow.get("permissions"), {"contents": "read"})
        self.assertEqual(workflow.get("concurrency", {}).get("group"), "emask-homebrew-publication")
        self.assertEqual(workflow.get("concurrency", {}).get("cancel-in-progress"), "false")

    def test_tap_uses_its_own_key_and_checks_run_before_pushing(self):
        steps = load("homebrew.yml").get("jobs", {}).get("publish", {}).get("steps", [])
        taps = [step for step in steps if step.get("with", {}).get("repository") == "dan-hart/homebrew-tap"]
        self.assertEqual(len(taps), 1)
        self.assertIn("HOMEBREW_TAP_SSH_KEY", taps[0]["with"]["ssh-key"])
        self.assertEqual(taps[0]["with"]["fetch-depth"], "0")
        runs = [step.get("run", "") for step in steps]
        prepare = next((i for i, run in enumerate(runs) if "homebrew.py prepare" in run), -1)
        publish = next((i for i, run in enumerate(runs) if "homebrew.py publish" in run), -1)
        self.assertGreaterEqual(prepare, 0)
        self.assertGreater(publish, prepare)
        for command in ("brew style", "brew audit --strict", "brew install", "brew test"):
            index = next((i for i, run in enumerate(runs) if command in run), -1)
            self.assertGreater(index, prepare)
            self.assertLess(index, publish)


if __name__ == "__main__":
    unittest.main()

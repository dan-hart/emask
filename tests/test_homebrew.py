# SPDX-License-Identifier: AGPL-3.0-or-later
import hashlib
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
import homebrew


def git(repo, *args):
    return subprocess.run(["git", "-C", str(repo), *args], check=True, capture_output=True, text=True).stdout.strip()


def formula(tag):
    entries = []
    for target in homebrew.TARGETS:
        entries.append(f'  url "https://github.com/dan-hart/emask/releases/download/{tag}/emask-{tag[1:]}-{target}.tar.gz"\n  sha256 "{"0" * 64}"')
    return 'class Emask < Formula\n  desc "Example"\n' + "\n".join(entries) + '\n  def install\n    bin.install "emask"\n  end\nend\n'


def init_repo(path, tag="v0.1.2"):
    path.mkdir()
    git(path, "init", "-b", "main")
    git(path, "config", "user.name", "Emask Tests")
    git(path, "config", "user.email", "tests@example.com")
    (path / "Formula").mkdir()
    (path / homebrew.FORMULA).write_text(formula(tag))
    (path / "README.md").write_text("Other packages remain unchanged.\n")
    git(path, "add", ".")
    git(path, "commit", "-m", "Initial tap")


class Fixture(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="emask-homebrew-test-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.assets = self.root / "assets"
        self.assets.mkdir()
        self.tag = "v0.1.3"
        self.metadata = {"tagName": self.tag, "isDraft": False, "isPrerelease": False}
        self.digests = {}
        for target in homebrew.TARGETS:
            name = f"emask-0.1.3-{target}.tar.gz"
            payload = f"synthetic {target} package".encode()
            (self.assets / name).write_bytes(payload)
            self.digests[name] = hashlib.sha256(payload).hexdigest()
        self.manifest = "".join(f"{digest}  {name}\n" for name, digest in self.digests.items())
        (self.assets / "SHA256SUMS").write_text(self.manifest)


class HomebrewTests(Fixture):
    def test_stable_version_and_numeric_order(self):
        self.assertEqual(homebrew.version_tuple("v1.10.0"), (1, 10, 0))
        self.assertGreater(homebrew.version_tuple("v1.10.0"), homebrew.version_tuple("v1.9.0"))

    def test_invalid_tags_are_rejected(self):
        for tag in ("1.2.3", "v01.2.3", "v1.2", "v1.2.3-beta", "v1.2.3/../../x", "--help", "v1.2.3\n"):
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                homebrew.version_tuple(tag)

    def test_draft_prerelease_mismatch_and_missing_metadata_are_rejected(self):
        for change in ({"isDraft": True}, {"isPrerelease": True}, {"tagName": "v0.1.4"}, {"isDraft": None}):
            with self.subTest(change=change), self.assertRaises(ValueError):
                homebrew.validate_release({**self.metadata, **change}, self.tag)

    def test_all_platform_assets_are_verified(self):
        self.assertEqual(homebrew.validate_assets(self.assets, self.tag), self.digests)

    def test_corrupted_archive_is_rejected(self):
        (self.assets / next(iter(self.digests))).write_bytes(b"corrupted")
        with self.assertRaisesRegex(ValueError, "checksum"):
            homebrew.validate_assets(self.assets, self.tag)

    def test_missing_archive_is_rejected(self):
        (self.assets / next(iter(self.digests))).unlink()
        with self.assertRaises(ValueError):
            homebrew.validate_assets(self.assets, self.tag)

    def test_symlink_archive_is_rejected(self):
        name = next(iter(self.digests))
        real = self.assets / name
        real.rename(self.root / "archive")
        real.symlink_to(self.root / "archive")
        with self.assertRaises(ValueError):
            homebrew.validate_assets(self.assets, self.tag)

    def test_duplicate_missing_malformed_and_traversal_manifest_entries_are_rejected(self):
        for manifest in (self.manifest + self.manifest.splitlines()[0] + "\n", "", "not a digest\n", self.manifest + f'{"0" * 64}  ../outside\n'):
            (self.assets / "SHA256SUMS").write_text(manifest)
            with self.subTest(manifest=manifest), self.assertRaises(ValueError):
                homebrew.validate_assets(self.assets, self.tag)

    def test_formula_update_preserves_other_details(self):
        source = formula("v0.1.2")
        updated = homebrew.render_formula(source, self.tag, self.digests)
        self.assertIn('/download/v0.1.3/', updated)
        self.assertNotIn('v0.1.2', updated)
        self.assertIn('bin.install "emask"', updated)
        self.assertIn('desc "Example"', updated)
        for digest in self.digests.values():
            self.assertIn(digest, updated)

    def test_missing_duplicate_or_mixed_platform_formula_is_rejected(self):
        source = formula("v0.1.2")
        for text in (source.replace(homebrew.TARGETS[0], "unknown"), source + source, source.replace('v0.1.2/emask-0.1.2-aarch64', 'v0.1.1/emask-0.1.1-aarch64')):
            with self.subTest(text=text), self.assertRaises(ValueError):
                homebrew.render_formula(text, self.tag, self.digests)

    def test_formula_downgrade_is_rejected(self):
        with self.assertRaisesRegex(ValueError, "downgrade"):
            homebrew.render_formula(formula("v0.2.0"), self.tag, self.digests)

    def test_prepare_changes_only_the_formula(self):
        tap = self.root / "tap"
        init_repo(tap)
        homebrew.prepare(tap, self.assets, self.tag, self.metadata)
        self.assertEqual(git(tap, "diff", "--name-only"), homebrew.FORMULA)
        self.assertIn('/download/v0.1.3/', (tap / homebrew.FORMULA).read_text())

    def test_failed_verification_does_not_modify_the_tap(self):
        tap = self.root / "tap"
        init_repo(tap)
        (self.assets / next(iter(self.digests))).unlink()
        with self.assertRaises(ValueError):
            homebrew.prepare(tap, self.assets, self.tag, self.metadata)
        self.assertEqual(git(tap, "status", "--porcelain"), "")

    def test_prepare_rejects_existing_local_changes(self):
        tap = self.root / "tap"
        init_repo(tap)
        (tap / "README.md").write_text("Unrelated user edits")
        with self.assertRaisesRegex(ValueError, "dirty"):
            homebrew.prepare(tap, self.assets, self.tag, self.metadata)


class PublicationTests(Fixture):
    def setUp(self):
        super().setUp()
        self.remote = self.root / "remote.git"
        subprocess.run(["git", "init", "--bare", "-b", "main", str(self.remote)], check=True, capture_output=True)
        seed = self.root / "seed"
        init_repo(seed)
        git(seed, "remote", "add", "origin", str(self.remote))
        git(seed, "push", "origin", "main")
        self.tap = self.clone("tap")
        homebrew.prepare(self.tap, self.assets, self.tag, self.metadata)

    def clone(self, name):
        path = self.root / name
        subprocess.run(["git", "clone", str(self.remote), str(path)], check=True, capture_output=True)
        git(path, "config", "user.name", "Emask Tests")
        git(path, "config", "user.email", "tests@example.com")
        return path

    def push_other_change(self, path="README.md", content="Concurrent package update\n"):
        competitor = self.clone("competitor")
        (competitor / path).write_text(content)
        git(competitor, "add", path)
        git(competitor, "commit", "-m", "Other tap update")
        git(competitor, "push", "origin", "main")

    def test_publishes_one_scoped_formula_commit(self):
        self.assertTrue(homebrew.publish(self.tap, self.assets, self.tag, self.metadata))
        self.assertIn('/download/v0.1.3/', git(self.remote, "show", f"main:{homebrew.FORMULA}"))
        self.assertEqual(git(self.remote, "diff", "--name-only", "HEAD~1", "HEAD"), homebrew.FORMULA)

    def test_rerun_proves_write_access_without_duplicate_commit(self):
        self.assertTrue(homebrew.publish(self.tap, self.assets, self.tag, self.metadata))
        before = git(self.remote, "rev-parse", "main")
        commands = []
        real_git = homebrew.git
        def recording_git(tap, *args):
            commands.append(args)
            return real_git(tap, *args)
        homebrew.prepare(self.tap, self.assets, self.tag, self.metadata)
        with patch.object(homebrew, "git", recording_git):
            self.assertFalse(homebrew.publish(self.tap, self.assets, self.tag, self.metadata))
        self.assertIn(("push", "--dry-run", "origin", "HEAD:main"), commands)
        self.assertEqual(before, git(self.remote, "rev-parse", "main"))

    def test_unrelated_local_changes_are_not_committed(self):
        (self.tap / "README.md").write_text("Unrelated local edit")
        with self.assertRaisesRegex(ValueError, "unrelated"):
            homebrew.publish(self.tap, self.assets, self.tag, self.metadata)
        self.assertNotIn('/download/v0.1.3/', git(self.remote, "show", f"main:{homebrew.FORMULA}"))

    def test_unrelated_unpublished_commits_are_not_pushed(self):
        (self.tap / "README.md").write_text("Unrelated local commit")
        git(self.tap, "add", "README.md")
        git(self.tap, "commit", "-m", "Unpublished user work")
        with self.assertRaisesRegex(ValueError, "unrelated"):
            homebrew.publish(self.tap, self.assets, self.tag, self.metadata)
        self.assertEqual(git(self.remote, "show", "main:README.md"), "Other packages remain unchanged.")

    def test_rebase_preserves_concurrent_other_package_update(self):
        self.push_other_change()
        self.assertTrue(homebrew.publish(self.tap, self.assets, self.tag, self.metadata))
        self.assertEqual(git(self.remote, "show", "main:README.md"), "Concurrent package update")
        self.assertIn('/download/v0.1.3/', git(self.remote, "show", f"main:{homebrew.FORMULA}"))

    def test_newer_formula_arriving_during_validation_is_preserved(self):
        self.push_other_change(homebrew.FORMULA, formula("v0.1.4"))
        with self.assertRaisesRegex(ValueError, "downgrade"):
            homebrew.publish(self.tap, self.assets, self.tag, self.metadata)
        self.assertIn('/download/v0.1.4/', git(self.remote, "show", f"main:{homebrew.FORMULA}"))

    def test_formula_changed_during_validation_requires_fresh_validation(self):
        self.push_other_change(homebrew.FORMULA, formula("v0.1.2").replace("Example", "Changed description"))
        with self.assertRaisesRegex(ValueError, "changed|conflict"):
            homebrew.publish(self.tap, self.assets, self.tag, self.metadata)

    def test_push_race_retries_and_preserves_other_update(self):
        real_git = homebrew.git
        raced = False
        def racing_git(tap, *args):
            nonlocal raced
            if args[0] == "push" and "--dry-run" not in args and not raced:
                raced = True
                self.push_other_change()
            return real_git(tap, *args)
        with patch.object(homebrew, "git", racing_git):
            self.assertTrue(homebrew.publish(self.tap, self.assets, self.tag, self.metadata))
        self.assertTrue(raced)
        self.assertEqual(git(self.remote, "show", "main:README.md"), "Concurrent package update")
        self.assertIn('/download/v0.1.3/', git(self.remote, "show", f"main:{homebrew.FORMULA}"))


if __name__ == "__main__":
    unittest.main()

#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Validate published emask assets and update only its Homebrew formula."""

import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys

TARGETS = ("aarch64-apple-darwin", "x86_64-apple-darwin", "x86_64-unknown-linux-gnu")
FORMULA = "Formula/emask.rb"
TAG = re.compile(r"v(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\.(0|[1-9][0-9]*)\Z")
ENTRY = re.compile(
    r'url "https://github\.com/dan-hart/emask/releases/download/'
    r'(?P<tag>v[0-9]+\.[0-9]+\.[0-9]+)/emask-'
    r'(?P<version>[0-9]+\.[0-9]+\.[0-9]+)-(?P<target>[^"/]+)\.tar\.gz"\n'
    r'(?P<indent>[ \t]+)sha256 "[0-9a-f]{64}"'
)


def version_tuple(tag):
    match = TAG.fullmatch(tag)
    if not match:
        raise ValueError("Expected a stable release tag in vX.Y.Z form")
    return tuple(int(part) for part in match.groups())


def validate_release(metadata, tag):
    version_tuple(tag)
    if not isinstance(metadata, dict) or metadata.get("tagName") != tag:
        raise ValueError("Published release tag does not match the requested tag")
    if metadata.get("isDraft") is not False or metadata.get("isPrerelease") is not False:
        raise ValueError("Only published stable releases can update Homebrew")


def validate_assets(assets, tag):
    version_tuple(tag)
    expected = {f"emask-{tag[1:]}-{target}.tar.gz" for target in TARGETS}
    manifest = assets / "SHA256SUMS"
    if not manifest.is_file() or manifest.is_symlink():
        raise ValueError("Missing regular SHA256SUMS file")
    digests = {}
    for line in manifest.read_text().splitlines():
        match = re.fullmatch(r"([0-9a-f]{64})  (emask-[^/\\\s]+\.tar\.gz)", line)
        if not match or match[2] not in expected or match[2] in digests:
            raise ValueError("Invalid, unexpected or duplicate checksum entry")
        digests[match[2]] = match[1]
    if set(digests) != expected:
        raise ValueError("Missing platform checksum in SHA256SUMS")
    for name, expected_digest in digests.items():
        path = assets / name
        if not path.is_file() or path.is_symlink():
            raise ValueError(f"Missing regular release archive: {name}")
        digest = hashlib.sha256()
        with path.open("rb") as archive:
            for chunk in iter(lambda: archive.read(1024 * 1024), b""):
                digest.update(chunk)
        if digest.hexdigest() != expected_digest:
            raise ValueError(f"Release archive checksum mismatch: {name}")
    return digests


def formula_version(text):
    entries = list(ENTRY.finditer(text))
    if len(entries) != len(TARGETS) or {entry["target"] for entry in entries} != set(TARGETS):
        raise ValueError("Formula must contain exactly the three supported platform URL/hash pairs")
    tags = {entry["tag"] for entry in entries}
    if len(tags) != 1 or any(entry["tag"] != f'v{entry["version"]}' for entry in entries):
        raise ValueError("Formula contains inconsistent release versions")
    return version_tuple(tags.pop())


def render_formula(text, tag, digests):
    if formula_version(text) > version_tuple(tag):
        raise ValueError("Refusing to downgrade the Homebrew formula")

    def entry(match):
        name = f'emask-{tag[1:]}-{match["target"]}.tar.gz'
        return (
            f'url "https://github.com/dan-hart/emask/releases/download/{tag}/{name}"\n'
            f'{match["indent"]}sha256 "{digests[name]}"'
        )

    return ENTRY.sub(entry, text)


def git(tap, *args):
    return subprocess.run(["git", "-C", str(tap), *args], check=True, capture_output=True, text=True).stdout.strip()


def prepare(tap, assets, tag, metadata):
    validate_release(metadata, tag)
    digests = validate_assets(assets, tag)
    if git(tap, "status", "--porcelain"):
        raise ValueError("Refusing to prepare a dirty Homebrew tap checkout")
    path = tap / FORMULA
    original = path.read_text()
    updated = render_formula(original, tag, digests)
    if updated != original:
        path.write_text(updated)
    print(f"Verified all three release archives; prepared {FORMULA} for {tag}")


def publish(tap, assets, tag, metadata):
    validate_release(metadata, tag)
    digests = validate_assets(assets, tag)
    changes = set(git(tap, "diff", "--name-only", "HEAD").splitlines())
    if changes - {FORMULA} or git(tap, "ls-files", "--others", "--exclude-standard"):
        raise ValueError("Refusing to publish unrelated local tap changes")
    path = tap / FORMULA
    tested = path.read_text()
    if render_formula(tested, tag, digests) != tested:
        raise ValueError("Formula is not prepared for these verified release assets")
    changed = bool(changes)
    if changed:
        git(tap, "add", "--", FORMULA)
        git(tap, "commit", "-m", f"emask {tag[1:]}")

    for attempt in range(3):
        git(tap, "fetch", "origin", "main")
        remote = git(tap, "rev-parse", "origin/main")
        unpublished = set(git(tap, "log", "--format=", "--name-only", "origin/main..HEAD").splitlines()) - {""}
        if unpublished - {FORMULA}:
            raise ValueError("Refusing to push unrelated unpublished tap commits")
        if formula_version(git(tap, "show", f"origin/main:{FORMULA}")) > version_tuple(tag):
            raise ValueError("Refusing to downgrade a newer formula published during validation")
        try:
            git(tap, "rebase", "origin/main")
        except subprocess.CalledProcessError as error:
            subprocess.run(["git", "-C", str(tap), "rebase", "--abort"], capture_output=True)
            raise ValueError("Tap formula changed or conflicted; rerun for fresh validation") from error
        if path.read_text() != tested:
            raise ValueError("Tap formula changed during validation; rerun the workflow")
        try:
            # A no-op replay must still authenticate a write operation.
            git(tap, "push", "--dry-run", "origin", "HEAD:main")
            git(tap, "push", "origin", "HEAD:main")
        except subprocess.CalledProcessError:
            git(tap, "fetch", "origin", "main")
            if attempt == 2 or git(tap, "rev-parse", "origin/main") == remote:
                raise
            print("Concurrent tap push detected; retrying with the latest main")
            continue
        result = "Published" if changed else "Already current; no duplicate commit for"
        print(f"Verified tap write access. {result} {tag}")
        return changed
    raise ValueError("Could not publish after three attempts")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("validate-tag")
    validate.add_argument("tag")
    for name in ("prepare", "publish"):
        command = commands.add_parser(name)
        command.add_argument("--tag", required=True)
        command.add_argument("--assets", type=Path, required=True)
        command.add_argument("--tap", type=Path, required=True)
        command.add_argument("--release-json", type=Path, required=True)
    args = parser.parse_args()
    try:
        if args.command == "validate-tag":
            version_tuple(args.tag)
            print(args.tag)
        else:
            metadata = json.loads(args.release_json.read_text())
            operation = prepare if args.command == "prepare" else publish
            operation(args.tap, args.assets, args.tag, metadata)
    except (ValueError, OSError, subprocess.CalledProcessError) as error:
        reason = error.stderr.strip() if isinstance(error, subprocess.CalledProcessError) else str(error)
        print(f"Homebrew publication failed: {reason}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())

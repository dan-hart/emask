# Homebrew Automation Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement inline. User approval covers implementation, deploy-key configuration, direct publication and a live current-release test.

**Goal:** Automatically update the Homebrew formula after stable app releases and verify the path against v0.1.2.

**Architecture:** A reusable GitHub Actions workflow downloads published assets, invokes a focused Python validator/updater, checks Homebrew on macOS, and publishes only the formula using a repository deploy key. A separate publish command safely handles no-op runs and concurrent tap writes.

**Tech Stack:** Python stdlib, unittest, Git, GitHub CLI/Actions, Homebrew, scoped SSH deploy key.

### Task 1: Asset validation and formula preparation

- [ ] Create `tests/test_homebrew.py` with failing fixtures for strict tags, stable metadata, all three checksum-verified assets, malformed/duplicate manifests, preservation of existing formula details and newer-version rejection.
- [ ] Implement `scripts/homebrew.py` with `validate-tag`, `prepare` and `publish` commands. Reject malformed source formulas and dirty tap checkouts. Modify only the known URL/hash pairs.
- [ ] Run `python3 -m unittest discover -s tests -p 'test_homebrew.py' -v` and confirm passing tests after observing expected initial failures.

### Task 2: Safe Git publication

- [ ] Add real bare-repository tests for a new version, idempotent rerun, preservation of unrelated files, a concurrent unrelated push, and a newer formula arriving during validation.
- [ ] Implement a scoped commit, dry-run write-access check, bounded fetch/rebase/push retry, and a check that rebasing never changes the exact tested formula.
- [ ] Verify local publication against a temporary clone and the existing release's downloaded archives, without writing to GitHub.

### Task 3: Workflow and documentation

- [ ] Add `.github/workflows/homebrew.yml` with workflow_call, stable release publication and manual tag retry triggers; serialize updates and require the deploy-key secret.
- [ ] Call it from `.github/workflows/release.yml` after the release job. Run style, strict audit, install and formula tests before publishing.
- [ ] Add YAML/wiring tests and Python automation tests to CI. Update `docs/homebrew.md`, `CONTRIBUTING.md` and relevant tap docs to describe automatic updates, retries and key rotation.
- [ ] Request focused code review, fix important issues, and pass local and GitHub checks before merging.

### Task 4: Credential and live verification

- [ ] Generate a private temporary ed25519 key, register its public key on the tap and upload its private key as `HOMEBREW_TAP_SSH_KEY` on emask. Never log either secret material or personal credentials; remove temporary files. Roll back a newly registered key if secret upload fails.
- [ ] Dispatch the merged Homebrew workflow for `v0.1.2`. Wait for success, inspect its check/push results and verify that the remote formula remains identical without a duplicate commit.
- [ ] Report the verified workflow and credential scope. Leave application version and installed service configuration unchanged.

# Setup Wizard Implementation Plan

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task. The user has authorized implementation and release in this session.

**Goal:** Make entering credentials for every supported service straightforward and release emask 0.1.1.

**Architecture:** Put the wizard in `src/setup.rs` with injectable prompt and service operations for deterministic tests. Reuse Config, ProviderConfig, Secret, and provider APIs. Keep command dispatch in `src/cli.rs` and avoid changing existing automation behavior.

**Tech Stack:** Rust, clap, rpassword, existing ureq APIs and TOML storage.

### Task 1: CLI entry point

- [x] Add failing parsing tests for `emask setup` and `emask setup fastmail --config FILE`.
- [x] Run `cargo test cli::tests` and confirm the setup subcommand is missing.
- [x] Add the setup command and module; retain existing mint/auth behavior.

### Task 2: Wizard

- [x] Add scripted tests for skipping, saving pasted DuckDuckGo tokens, preserving existing credentials and metadata, verified Fastmail save, failed verification retry/skip, email login, defaults, ambiguous aliases, EOF, and per-account persistence.
- [x] Implement target selection, interactive-only terminal UI with hidden secrets, and provider operations in `src/setup.rs`.
- [x] Verify Fastmail candidates before inserting into Config. Never emit credentials in summaries or errors; use a generic service failure message in the wizard if an upstream error might contain sensitive data.
- [x] Offer retry/skip on service errors and blank-token skip. Persist successful accounts individually and choose an enabled default with a token. Preserve valid defaults and existing disabled state unless explicitly changed.
- [x] Run `cargo test`; fix failures and add regression tests for discovered edge cases.

### Task 3: Documentation and patch release

- [x] Put `emask setup` first in CLI help and no-configuration hints. Update README and SECURITY.md for wizard input, verification, skipping, reruns and automation.
- [x] Bump Cargo.toml and Cargo.lock to 0.1.1 and add the dated changelog entry.
- [x] Run `cargo fmt --all --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`, and `cargo build --release`.
- [x] Exercise the release binary via a pseudo-terminal with an isolated config and synthetic tokens, including all-skipped, token-hidden, existing-account and EOF paths.
- [x] Request code review, address important findings, rerun relevant checks.
- [ ] Commit and push to the existing release branch, wait for CI, tag v0.1.1 and publish a GitHub release with notes. Verify remote tag, release, version and clean worktree.

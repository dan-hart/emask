# Contributing

Thanks for looking. emask is small on purpose; please keep it that way.

## Install and develop

For the released CLI, use `brew install dan-hart/tap/emask`, then `emask setup`.
The [Homebrew guide](docs/homebrew.md) covers upgrades, installed paths,
completions, and moving from Cargo.

For development, install Rust 1.88 or newer, clone this repository, and use
`cargo run -- --help` or `cargo run -- setup`. This runs your checkout without
replacing the Homebrew installation. See the [README](README.md#install) for
Linux clipboard build dependencies.

## Ground rules

- **No secrets, no personal data in the repo.** Tests and docs use
  placeholders (`example.com`, `u12345678`, `alice`). Never paste a real
  address, account id, or token — not even a revoked one.
- **Tokens only through `Secret`.** Anything that holds a credential uses
  `crate::secret::Secret`. Reach for `.expose()` only at the HTTP call site.
- `cargo fmt`, `cargo clippy --all-targets -- -D warnings`, and `cargo test`
  must pass; CI runs them on Linux and macOS.

## Adding a provider

1. Add a variant to `providers::Kind` with its aliases, display name, token
   help, and setup hint.
2. Create `src/providers/<name>.rs` with `create(name, cfg, opts) -> Result<Mask>`
   and `check(name, cfg) -> Result<String>`. Use `http::agent()` and map errors
   with `http::describe`. Keep response parsing in a pure function with tests.
3. Wire the two match arms in `providers::create` and `providers::check`.
4. Document the token source in `README.md`.

## Releasing

1. Bump `version` in `Cargo.toml` (and `Cargo.lock` via `cargo build`).
2. Add a `## X.Y.Z — YYYY-MM-DD` section to `CHANGELOG.md`.
3. Merge to `main`, then tag and push:

   ```bash
   git tag -a vX.Y.Z -m "emask X.Y.Z"
   git push origin vX.Y.Z
   ```

The `Release` workflow builds macOS and Linux binaries, checks that the tag
matches `Cargo.toml`, and publishes a GitHub release whose notes are the
matching `CHANGELOG.md` section.

After the release workflow finishes, update
[`Formula/emask.rb` in dan-hart/homebrew-tap](https://github.com/dan-hart/homebrew-tap/blob/main/Formula/emask.rb)
with the new release URLs and verified SHA-256 hashes for all three platforms.
Run Homebrew style, strict audit, installation, and formula tests before
publishing the tap update. This is a separate step: the release workflow does
not currently update the tap. Follow the full [Homebrew release checklist](docs/homebrew.md#publishing-a-homebrew-update),
including updating versioned install examples in the README.

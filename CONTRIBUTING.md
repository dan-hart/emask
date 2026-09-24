# Contributing

Thanks for looking. emask is small on purpose; please keep it that way.

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

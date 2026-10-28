# Automatic Homebrew publication

The user approved automatic direct tap updates, a dedicated deploy key, and a
live test against v0.1.2. Keep the application version unchanged.

Use a reusable `homebrew.yml` workflow called after the Release publisher
succeeds. Also accept `release: published` for manually published stable
releases, and `workflow_dispatch` with a stable `vX.Y.Z` tag for retries.
The explicit call is necessary because release events created using
`GITHUB_TOKEN` do not trigger another workflow.

Run publication on macOS with a read-only application token and a dedicated
`HOMEBREW_TAP_SSH_KEY` secret. Its public key belongs only to
`dan-hart/homebrew-tap`, with write access. Do not reuse Momentum's key or any
personal token. Create the key in a private temporary directory, upload it
directly to GitHub Secrets, and delete the temporary private file.

Download the published release metadata, three platform archives and
`SHA256SUMS`. Validate a stable matching tag, reject draft/prerelease metadata,
and verify every required archive before changing the formula. Update only the
three versioned URLs and hashes in `Formula/emask.rb`, retaining its install,
documentation, completions, caveats and tests.

Serialize emask publishers without canceling an active publish. Prevent older
versions from overwriting newer formulas. Run Homebrew style, strict audit,
installation and tests on the prepared formula before committing. Push only
`Formula/emask.rb`; use bounded fetch/rebase retries to preserve unrelated tap
updates such as Momentum. Stop if the formula changes during validation or a
newer version arrives. A rerun with matching contents must prove write access
with a dry-run push and finish without another commit.

Use stdlib Python tests with real temporary Git repositories and synthetic
assets. Cover corrupted/missing/duplicate checksums, invalid tags, unstable
releases, preservation of other files, downgrades, no-op publication, and
concurrent tap writes. Validate workflow trigger, dependency and secret wiring.
Update contributor and Homebrew guides, then merge after CI and review. Run
the live workflow on v0.1.2; verify checks, SSH write access and unchanged tap
formula. Do not mint addresses or use service tokens.

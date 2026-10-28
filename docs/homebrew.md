# Homebrew installation and upgrades

The maintained formula is [dan-hart/tap/emask](https://github.com/dan-hart/homebrew-tap/blob/main/Formula/emask.rb).
It installs the published binary for Apple Silicon Macs, Intel Macs, or Linux
x86_64. A Rust toolchain is not needed.

## Install and set up

```bash
brew install dan-hart/tap/emask
emask --version
emask setup
```

Homebrew adds `dan-hart/tap` automatically. The setup wizard guides you through
Fastmail and DuckDuckGo credentials; see the [quick start](../README.md#quick-start).
The formula also installs Bash, Zsh, and Fish completions.

## Upgrade

```bash
brew update
brew upgrade dan-hart/tap/emask
emask --version
```

Existing service tokens and preferences stay in `~/.config/emask/config.toml`
(or the location selected by `--config` / `$EMASK_CONFIG`). Run `emask setup`
again only when you want to add or replace credentials.

## Installation paths and completions

Use `command -v emask` to see which binary your shell runs, and
`brew --prefix emask` to find the installed package. The executable is linked
under `$(brew --prefix)/bin`: normally `/opt/homebrew/bin/emask` on Apple Silicon
or `/usr/local/bin/emask` on an Intel Mac.

Homebrew installs completions automatically; the manual completion commands in
the [README](../README.md#install) are for other installation methods. If your
shell cannot find emask, follow Homebrew's `brew shellenv` instructions for your
shell and open a new terminal.

Apple Shortcuts uses a minimal `PATH`. The current [Shortcuts generator](shortcuts.md)
also checks both standard Mac Homebrew paths explicitly. Regenerate Shortcuts
created with older versions of the generator after moving from Cargo.

## Moving from Cargo

Install Homebrew's package first, then verify it directly:

```bash
brew install dan-hart/tap/emask
"$(brew --prefix)/bin/emask" --version
command -v emask
```

If `command -v emask` still points to `~/.cargo/bin/emask` and you originally
installed it with Cargo, remove that older package:

```bash
cargo uninstall emask
hash -r
command -v emask
emask --version
```

Your tokens stay in the same configuration file. For manually copied binaries,
adjust your `PATH` or move the old executable out of it after verifying the
Homebrew installation.

## Publishing a Homebrew update

The emask release workflow publishes archives but does not update the tap.
Maintainers must complete this step for every new version:

1. Wait for the tagged GitHub release to publish all three binary archives and
   `SHA256SUMS`. Download and verify them with `shasum -a 256 -c SHA256SUMS`.
2. Update each platform's URL and `sha256` in the tap's `Formula/emask.rb`:
   `aarch64-apple-darwin`, `x86_64-apple-darwin`, and `x86_64-unknown-linux-gnu`.
   Use the release hashes without rebuilding or modifying the archives.
3. In a local checkout of the installed tap, run:

   ```bash
   brew style dan-hart/tap/emask
   brew audit --strict dan-hart/tap/emask
   brew install dan-hart/tap/emask    # or brew upgrade if already installed
   brew test dan-hart/tap/emask
   "$(brew --prefix)/bin/emask" --version
   ```

4. Commit and push the validated formula to `dan-hart/homebrew-tap`, then verify
   its published content matches the tested formula. Keep the README's pinned
   source-install tag and all install/upgrade examples consistent with the release.

Use the [contributor release steps](../CONTRIBUTING.md#releasing) to publish the
app release first. Homebrew package tests must not mint addresses or require real
service credentials.

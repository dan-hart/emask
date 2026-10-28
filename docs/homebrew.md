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

Stable releases update the tap automatically. After the Release workflow
publishes the archives, it calls the [Homebrew workflow](../.github/workflows/homebrew.yml).
That workflow also listens for manually published releases. The explicit call
is required because [events created by `GITHUB_TOKEN` normally do not start
another workflow](https://docs.github.com/en/actions/how-tos/write-workflows/choose-when-workflows-run/trigger-a-workflow#triggering-a-workflow-from-a-workflow).

The publisher accepts stable `vX.Y.Z` tags only, rejects draft/prerelease
metadata, and verifies all three archives against `SHA256SUMS` before changing
the formula. On a Mac runner it checks the prepared formula with `brew style`,
`brew audit --strict`, `brew install` and `brew test`, then commits only
`Formula/emask.rb`. Existing install steps, documentation, completions and
formula tests are preserved. It never mints an address or requires service tokens.

Repeated runs with identical release contents finish without another commit,
but still verify SSH write access. Older releases cannot downgrade the tap.
Emask publishers are serialized, and bounded fetch/rebase retries preserve
concurrent changes to other tap packages. If the emask formula itself changes
during validation, the job stops and requires a fresh run.

### Retry or verify an existing release

After a transient download, runner, or Git push failure, rerun the failed job
in Actions or dispatch a fresh run with the published stable tag:

```bash
gh workflow run homebrew.yml --repo dan-hart/emask --ref main -f tag=v0.1.2
gh run list --repo dan-hart/emask --workflow homebrew.yml
```

The job uses publication tooling from `main`, so this also works for releases
created before the automation existed. Missing archives, checksum failures and
credential failures stop publication with an error; resolve the cause before
retrying. If a newer formula is already published, retry that newer tag instead.

### Deploy-key configuration and rotation

The `HOMEBREW_TAP_SSH_KEY` Actions secret on `dan-hart/emask` holds a dedicated
ed25519 private key. Its public key is registered as **emask automatic Homebrew
releases** on `dan-hart/homebrew-tap`, with write access to that repository.
It is separate from Momentum's deploy key and from personal credentials.
The application's `GITHUB_TOKEN` has read-only access in the Homebrew workflow;
tap writes use the deploy key.

To rotate, generate a new key in a private temporary directory, add its public
key as a write-enabled deploy key on the tap, and upload the private key
directly with `gh secret set HOMEBREW_TAP_SSH_KEY --repo dan-hart/emask < KEY_FILE`.
Never commit the key or pass it as a command-line argument. Delete the local
private key after uploading. Dispatch a verification run against the latest
stable tag, then remove the old deploy key after the new run succeeds.

### Manual recovery

If Actions is unavailable, download the published archives and `SHA256SUMS`,
verify them with `shasum -a 256 -c SHA256SUMS`, and update all three platform
URL/hash pairs in a clean tap checkout. Run style, strict audit, installation
and formula tests before pushing only `Formula/emask.rb`. Preserve newer
versions and unrelated tap changes. Prefer the workflow for ordinary releases.

Use the [contributor release steps](../CONTRIBUTING.md#releasing) to publish the
app first, and keep the README's pinned source-install tag current.

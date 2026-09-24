# emask

[![CI](https://github.com/dan-hart/emask/actions/workflows/ci.yml/badge.svg)](https://github.com/dan-hart/emask/actions/workflows/ci.yml)
[![License: AGPL-3.0-or-later](https://img.shields.io/badge/license-AGPL--3.0--or--later-blue.svg)](LICENSE)

Mint a masked email address from the terminal and have it on your clipboard
before the signup form finishes loading.

```console
$ emask fm --for example.com
quiet.harbor1234@fastmail.com
copied to clipboard

$ emask ddg
brave-otter-quiet@duck.com
copied to clipboard
```

- **Fast.** A single static binary, one HTTP request per address, no runtime.
- **Simple.** `emask <provider>`. That is the whole hot path.
- **Portable.** Every provider and token lives in one TOML file. Copy it to a
  new machine (or `emask config export` / `import`) and you are done.
- **Careful with secrets.** Tokens are prompted with hidden input, stored
  `0600`, never printed unless you ask with `--reveal`, redacted in every
  debug path, and zeroed in memory when dropped. See [SECURITY.md](SECURITY.md).

Providers today: [Fastmail Masked Email](#fastmail) and
[DuckDuckGo Email Protection](#duckduckgo). Adding one is a small,
well-marked change — see [CONTRIBUTING.md](CONTRIBUTING.md).

## Install

Prebuilt binaries for macOS (Apple silicon and Intel) and Linux x86_64 are on
the [releases page](https://github.com/dan-hart/emask/releases), each with a
`SHA256SUMS` file. Unpack and put `emask` somewhere on your `PATH`.

Or build from source (Rust 1.88 or newer):

```bash
cargo install --git https://github.com/dan-hart/emask --tag v0.1.0
```

or clone and `cargo install --path .`. The binary lands in `~/.cargo/bin/emask`.
On Linux the clipboard needs the X11 `xcb` development headers at build time
(`libxcb1-dev libxcb-render0-dev libxcb-shape0-dev libxcb-xfixes0-dev` on Debian/Ubuntu).

Shell completions:

```bash
emask completions zsh  > ~/.zfunc/_emask      # zsh (ensure ~/.zfunc is in fpath)
emask completions bash > ~/.local/share/bash-completion/completions/emask
emask completions fish > ~/.config/fish/completions/emask.fish
```

## Quick start

### Fastmail

1. Create an API token at <https://app.fastmail.com/settings/security/tokens/new>
   with **only** the *Masked Email* scope.
2. Store it (you will be prompted; input is hidden and never echoed):

   ```bash
   emask auth set fm
   ```

   emask immediately verifies the token against Fastmail's session endpoint
   and caches your masked-email account id.

3. Mint:

   ```bash
   emask fm                        # description "emask"
   emask fm --for shop.example     # Fastmail shows the site in its Masked Email list
   emask fm --for shop.example --desc "Order tracking" --prefix shop
   ```

### DuckDuckGo

You need a Duck Address (`you@duck.com`). Log in once with the one-time
passphrase DuckDuckGo emails you:

```bash
emask auth login ddg --user you
# DuckDuckGo emailed a one-time passphrase to the inbox behind you@duck.com.
# Paste the passphrase (words separated by spaces): ····
# logged in; access token saved for 'ddg'
emask ddg
```

If you already have an access token (the DuckDuckGo browser exposes it under
*Email Protection → Autofill*), `emask auth set ddg` stores it directly.

### Pick a default

```bash
emask default fm      # now plain `emask` uses Fastmail
emask                 # → a Fastmail address
```

## Everyday use

| Command | What it does |
|---|---|
| `emask <PROVIDER>` | Mint an address, print it, copy it |
| `emask <PROVIDER> --for SITE` | Label it with the site (Fastmail stores this; DuckDuckGo cannot) |
| `emask <PROVIDER> --json` | `{"email","provider","kind","copied","note"}` for scripts |
| `emask <PROVIDER> --no-copy` | Leave the clipboard alone (useful in pipelines) |
| `emask providers` | Table of providers: kind, enabled, token present, default |
| `emask enable` / `disable NAME` | Toggle a provider without deleting its token |
| `emask check [NAME]` | Verify credentials without minting (Fastmail: live; DuckDuckGo: presence only) |

`PROVIDER` is either a name you chose (`fm`, `ddg`, `work`, …) or a kind
alias (`fastmail`/`fm`, `duckduckgo`/`ddg`). An alias resolves when exactly one
provider of that kind exists (or exactly one is enabled).

Plain-text output is just the address on stdout; status lines go to stderr, so
`emask fm | pbcopy` or `$(emask ddg --no-copy)` behave.

## Configuration

One file: `~/.config/emask/config.toml` (override with `--config FILE` or
`$EMASK_CONFIG`). emask writes it with mode `0600` inside a `0700` directory
and warns if it ever finds it readable by others.

```toml
version = 1
default = "fm"

[providers.fm]
kind = "fastmail"
enabled = true
token = "fmu1-…"
account_id = "u12345678"        # cached automatically

[providers.ddg]
kind = "duckduckgo"
enabled = true
token = "…"
username = "you"

[providers.work]
kind = "fastmail"
enabled = false                 # kept around, refused when minting
token = "fmu1-…"
description = "Work signups"    # default description for this provider
```

### Moving to another machine

```bash
# old machine
emask config export ~/emask.toml        # written 0600; contains tokens
# new machine
emask config import ~/emask.toml        # replaces the config
emask config import ~/emask.toml --merge   # or add to what is there
```

Copying the file by hand to `~/.config/emask/config.toml` works too — it is
the same file. `emask config path` prints where emask is looking.

Other config commands: `emask config show` (tokens redacted; `--reveal` to
print them), `emask config edit` (opens `$VISUAL`/`$EDITOR`, then validates).

## Automation

Because output is a bare address on stdout and the clipboard copy happens
in-process, emask drops into anything that can run a command:

- **Apple Shortcuts / Raycast / Alfred:** see [docs/shortcuts.md](docs/shortcuts.md)
  for ready-made Shortcuts that wrap `emask ddg` and `emask fm --for …`.
- **Shell:** `alias mask='emask fm --for'`
- **Scripts:** `emask ddg --json --no-copy | jq -r .email`

## Exit status

`0` on success. `1` on any failure, with a one-line reason on stderr and a
hint for the common cases (missing token, rejected token, disabled provider).

## Security

Read [SECURITY.md](SECURITY.md) for the threat model, what emask does with
tokens, and how to report a problem.

## License

AGPL-3.0-or-later. See [LICENSE](LICENSE).

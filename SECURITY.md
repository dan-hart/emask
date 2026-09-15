# Security

emask exists to hold API tokens and use them on your behalf, so its whole job
is to not leak them.

## What emask does with tokens

- **Input.** `emask auth set` reads the token from a hidden terminal prompt,
  from a file (`--token-file`, first line only), or from stdin when piped.
  `--token` on the command line is accepted for scripting but prints a warning
  because it is visible in shell history and `ps`.
- **Storage.** Tokens live only in the config file, written with mode `0600`
  inside a `0700` directory. On load, emask warns if the file is group- or
  world-readable. No keychain, no cache, no other copy.
- **In memory.** Every token is a `Secret` newtype: `Debug` prints a redacted
  form, there is no `Display`, and the buffer is zeroed on drop. Raw values are
  only reachable through `Secret::expose`, which keeps every use greppable.
- **Output.** Tokens are never printed unless you pass `--reveal` to
  `emask auth show` or `emask config show`. `emask config export` writes the
  full config (that is its purpose) with mode `0600` and notes on stderr when
  it is printing to a terminal.
- **Network.** Tokens are sent only as `Authorization: Bearer` headers to the
  provider's own API host over HTTPS (`api.fastmail.com`,
  `quack.duckduckgo.com`). They are never placed in URLs or logged. Error
  messages include the HTTP status and a trimmed response body, never the
  request.
- **Scope.** Fastmail: use a token with only the *Masked Email* scope; emask
  refuses tokens without it and does nothing that needs more. DuckDuckGo: the
  access token can mint addresses and read the Email Protection dashboard;
  emask only mints.

## What emask does not protect against

- Anyone who can read your home directory as your user can read the config.
  That is the same trust boundary as `~/.ssh` or `~/.aws`.
- A compromised provider API or TLS interception on your machine.
- Memory disclosure while the process is running (tokens are in RAM briefly).

## Reporting a vulnerability

Please open a private security advisory on GitHub
(<https://github.com/dan-hart/emask/security/advisories/new>) rather than a
public issue. Include the emask version (`emask --version`) and reproduction
steps. You should hear back within a week.

# Changelog

## 0.1.1 — 2026-10-28

- Add `emask setup`: a guided terminal wizard for Fastmail and DuckDuckGo,
  with service instructions, hidden token prompts, and optional DuckDuckGo
  email passphrase login.
- Skip services, keep existing credentials by default, and safely retry failed
  checks. Fastmail replacements are verified before saving; pasted DuckDuckGo
  tokens are clearly marked as unverified. Setup never creates an address.
- Select a default service and get a ready-to-run command after setup. Re-run
  the wizard for all services or just one named account.
- Make guided setup discoverable in CLI help, first-run messages, and docs;
  credential hints now recommend hidden prompts instead of command-line tokens.
- Fix global `--config FILE` handling before subcommands, and exercise real
  terminal setup with synthetic credentials in Linux and macOS CI.

## 0.1.0 — 2026-09-15

Initial release.

- Providers: Fastmail Masked Email (JMAP) and DuckDuckGo Email Protection.
- `emask <provider>` mints, prints, and copies an address; `--for`, `--desc`,
  `--prefix`, `--json`, `--no-copy`.
- Named providers with enable/disable and a default; kind aliases `fm`/`ddg`.
- Portable single-file TOML config with `export`/`import`/`edit`/`show`.
- Credential handling: hidden prompts, `--token-file`, `0600` storage,
  redaction everywhere, zeroize on drop, permission warnings.
- DuckDuckGo one-time-passphrase login (`emask auth login`).
- Shell completions; Apple Shortcuts wrappers in `contrib/shortcuts`.

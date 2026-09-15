# Changelog

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

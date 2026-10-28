# Guided service setup

Approved by the user on 2026-10-28, including implementation and a patch release.

Add `emask setup [PROVIDER]`. Without a provider, walk through Fastmail and
DuckDuckGo, including existing named accounts; with a provider, configure just
that account (existing name or an unambiguous service alias). Show credential
instructions, accept hidden token input, and let users skip either service.
DuckDuckGo additionally offers its existing email passphrase login, with hidden
passphrase input. No address is minted during setup.

Existing credentials require an explicit replacement choice. A blank token skips
an account. Verify Fastmail before saving; errors offer retry or skip, preserving
the old account. DuckDuckGo pasted tokens are described as saved, not verified.
Save each completed account so later cancellation does not lose earlier work.
Keep metadata and disabled status unless the user explicitly enables an account.
Choose a default from enabled accounts with tokens, preserving a valid existing
default. Finish with the config path and a command to mint an address.

Use the existing private TOML storage and Secret wrapper. Reject redirected
input for the wizard with actionable guidance to existing scriptable auth
commands. EOF/cancellation exits without writing the account in progress.
Update CLI help, empty-config hints, README, security docs, and changelog.

Verify using scripted wizard tests with injected UI and provider operations,
CLI parsing tests, private-file persistence tests, a real-terminal smoke test,
formatting, clippy, the complete test suite, and a release build. Publish version
0.1.1 using the repository's version/changelog/tag release convention and a
GitHub release after CI succeeds. Do not use real credentials in tests.

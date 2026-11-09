# Apple Shortcuts wrappers

Two Shortcuts that turn emask into a menu-bar / Spotlight / hotkey action on
macOS. They are thin: a *Run Shell Script* action that calls `emask`, and a
notification showing the address. emask does the copying.

| Shortcut | Runs |
|---|---|
| **DDG Masked Email** | `emask ddg` |
| **Fastmail Masked Email** | asks *"Which site is this for?"* → `emask fm --for <answer>` |

## Build and install

Install emask and connect your services in a terminal first:

```bash
brew install dan-hart/tap/emask
emask setup
```

Homebrew installs shell completions too. To update emask later, run
`brew update && brew upgrade dan-hart/tap/emask`. See the
[Homebrew guide](homebrew.md) for installation paths and moving from Cargo.

Then clone this repository and generate the Shortcuts from its root directory:

```bash
python3 contrib/shortcuts/build.py /tmp/emask-shortcuts
cd /tmp/emask-shortcuts
for n in "DDG Masked Email" "Fastmail Masked Email"; do
  shortcuts sign --mode anyone --input "$n.shortcut" --output "$n.signed.shortcut"
  open "$n.signed.shortcut"      # Shortcuts opens; click "Add Shortcut"
done
```

Once added you can run them from the Shortcuts menu-bar item, Spotlight, a
keyboard shortcut (Shortcut → ⓘ → *Add Keyboard Shortcut*), or the CLI:

```bash
shortcuts run "DDG Masked Email"
```

## Requirements

- `emask` installed with `brew install dan-hart/tap/emask` or another method,
  with providers configured using `emask setup` (`emask providers` lists them).
  Generated scripts check `PATH`, then `/opt/homebrew/bin/emask` (Apple Silicon),
  `/usr/local/bin/emask` (Intel Mac), and `~/.cargo/bin/emask` (Cargo).
- Shortcuts → Settings → Advanced → **Allow Running Scripts** turned on.
- The first run asks you to allow the shortcut to run scripts; choose *Always Allow*.

## Why not call the APIs from Shortcuts directly?

You can, but then tokens live inside the shortcut (synced through iCloud,
visible in the editor) and every fix means editing actions by hand. Keeping the
logic in emask means one config file, redacted secrets, real error messages,
and the same behaviour from a terminal, Raycast, Alfred, or Shortcuts.

## iPhone over Tailscale

**New Masked Email** copies a fresh alias to the iPhone clipboard. Launch it
directly to use the host's configured default provider. In Safari, choose
**Share → New Masked Email** to use DuckDuckGo (`ddg`). The Shortcut uses the
presence of a shared Safari webpage to choose the provider and sends only
the provider choice to the host.

### Set up the host

On an always-on Tailscale device, install Emask and Python 3.9 or later. Run
`emask setup` as the user that will run the server. Make sure that user has a
default provider and a working DuckDuckGo provider named `ddg`.

```bash
python3 contrib/tailnet/serve.py --allow-login YOUR_TAILSCALE_LOGIN
```

Use the login shown for your iPhone in the Tailscale admin console. The server
listens only on `127.0.0.1:8770`. Use `--emask /path/to/emask` if your service
manager runs with a restricted `PATH`. Keep the process running with your
host's service manager for daily use.

In another terminal, publish the API on an unused HTTPS port (8447 in this
example). `tailscale serve status` shows the origin to give the Shortcut.

```bash
tailscale serve --bg --https=8447 http://127.0.0.1:8770
tailscale serve status
```

The API trusts the login header that [Tailscale Serve](https://tailscale.com/docs/features/tailscale-serve)
injects after removing any client-supplied value. Keep the backend on loopback
and configure it behind Serve. The configured login can call the API from
devices on the tailnet that your access policy permits. To remove this route,
run `tailscale serve --https=8447 off` and stop the backend.

### Install the Shortcut

On a Mac, substitute your Serve origin in the following commands:

```bash
python3 contrib/shortcuts/build.py /tmp/emask-shortcuts \
  --tailnet-url https://emask.example.ts.net:8447
shortcuts sign --mode anyone \
  --input '/tmp/emask-shortcuts/New Masked Email.shortcut' \
  --output '/tmp/New Masked Email.shortcut'
```

Open the signed file on your iPhone or import it on your Mac and let
Shortcuts sync it to the iPhone. Connect Tailscale on the phone and allow
the Shortcut to access the Serve host when prompted. Run it directly or
from Safari's share sheet, then paste into the email field.

### API

Send `POST /v1/aliases` with `Content-Type: application/json` and `{}` for the
configured default or `{"provider":"ddg"}` for DuckDuckGo. A successful request
returns `201` with `{"email":"…"}`. Each successful request creates an address.

## Troubleshooting

- **"emask not found"** — install with `brew install dan-hart/tap/emask`.
  Shortcuts uses a minimal `PATH`; regenerate older Shortcuts with the current
  script to include Homebrew paths. For a custom installation, set `EMASK` to
  the absolute path printed by `command -v emask` in the shortcut's shell script.
- **Wrong version runs** — run `command -v emask` and `emask --version` in a
  terminal; see [moving from Cargo](homebrew.md#moving-from-cargo). Regenerate
  older Shortcuts that explicitly point to the Cargo binary.
- **Notification shows an error** — it is emask's own message; run the same
  command in a terminal (`emask ddg`) for the full text and hints.
- **Nothing happens** — check Shortcuts' *Allow Running Scripts* setting.

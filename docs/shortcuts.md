# Apple Shortcuts wrappers

Two Shortcuts that turn emask into a menu-bar / Spotlight / hotkey action on
macOS. They are thin: a *Run Shell Script* action that calls `emask`, and a
notification showing the address. emask does the copying.

| Shortcut | Runs |
|---|---|
| **DDG Masked Email** | `emask ddg` |
| **Fastmail Masked Email** | asks *"Which site is this for?"* → `emask fm --for <answer>` |

## Build and install

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

- `emask` installed (the script looks in `~/.cargo/bin/emask`, then `PATH`)
  with providers configured (`emask providers`).
- Shortcuts → Settings → Advanced → **Allow Running Scripts** turned on.
- The first run asks you to allow the shortcut to run scripts; choose *Always Allow*.

## Why not call the APIs from Shortcuts directly?

You can, but then tokens live inside the shortcut (synced through iCloud,
visible in the editor) and every fix means editing actions by hand. Keeping the
logic in emask means one config file, redacted secrets, real error messages,
and the same behaviour from a terminal, Raycast, Alfred, or Shortcuts.

## Troubleshooting

- **"emask not found"** — install with `cargo install --path .` or edit the
  script line `EMASK=...` in the shortcut.
- **Notification shows an error** — it is emask's own message; run the same
  command in a terminal (`emask ddg`) for the full text and hints.
- **Nothing happens** — check Shortcuts' *Allow Running Scripts* setting.

#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate Apple Shortcuts that wrap emask.

    python3 contrib/shortcuts/build.py [OUTPUT_DIR]

Produces two unsigned .shortcut plists:

  "DDG Masked Email"       runs `emask ddg`, shows a notification
  "Fastmail Masked Email"  asks which site the address is for, runs `emask fm --for <site>`

Then sign and open them (macOS 12+):

    shortcuts sign --mode anyone --input "DDG Masked Email.shortcut" --output "DDG Masked Email.signed.shortcut"
    open "DDG Masked Email.signed.shortcut"      # click "Add Shortcut"

Requirements: emask installed at ~/.cargo/bin/emask (or on PATH) and
Shortcuts → Settings → Advanced → "Allow Running Scripts" enabled.
The shortcut's shell script is deliberately tiny; all logic lives in emask.
"""
import os
import plistlib
import sys
import uuid

OUT = sys.argv[1] if len(sys.argv) > 1 else "."
os.makedirs(OUT, exist_ok=True)

# Shortcuts runs scripts with a minimal PATH; find emask explicitly.
PRELUDE = (
    'EMASK="$HOME/.cargo/bin/emask"\n'
    '[ -x "$EMASK" ] || EMASK="$(command -v emask)" || { echo "emask not found" >&2; exit 1; }\n'
)


def uid():
    return str(uuid.uuid4()).upper()


def attach(out_uuid, name):
    return {
        "Value": {"OutputUUID": out_uuid, "Type": "ActionOutput", "OutputName": name},
        "WFSerializationType": "WFTextTokenAttachment",
    }


def text(parts):
    """A Shortcuts rich-text value: str pieces and (uuid, name) variable pieces."""
    s, ranges = "", {}
    for p in parts:
        if isinstance(p, str):
            s += p
        else:
            ranges["{%d, 1}" % len(s)] = {"OutputUUID": p[0], "Type": "ActionOutput", "OutputName": p[1]}
            s += "￼"
    return {"Value": {"string": s, "attachmentsByRange": ranges}, "WFSerializationType": "WFTextTokenString"}


def action(identifier, params):
    params = dict(params)
    params.setdefault("UUID", uid())
    return {"WFWorkflowActionIdentifier": identifier, "WFWorkflowActionParameters": params}


def shell(script, stdin_from=None):
    p = {"Script": PRELUDE + script, "Shell": "/bin/zsh", "InputMode": "to stdin", "ShowWhenRun": False}
    if stdin_from is not None:
        p["Input"] = attach(*stdin_from)
    return action("is.workflow.actions.runshellscript", p)


def notify(title, result):
    return action(
        "is.workflow.actions.notification",
        {"WFNotificationActionTitle": title, "WFNotificationActionBody": text([result]), "WFNotificationActionSound": False},
    )


def workflow(actions, glyph, color):
    return {
        "WFWorkflowMinimumClientVersion": 900,
        "WFWorkflowMinimumClientVersionString": "900",
        "WFWorkflowClientVersion": "2607.1.3",
        "WFWorkflowIcon": {"WFWorkflowIconStartColor": color, "WFWorkflowIconGlyphNumber": glyph},
        "WFWorkflowImportQuestions": [],
        "WFWorkflowTypes": ["MenuBar", "QuickActions"],
        "WFWorkflowInputContentItemClasses": [],
        "WFWorkflowHasOutputFallback": False,
        "WFWorkflowHasShortcutInputVariables": False,
        "WFWorkflowActions": actions,
    }


def uuid_of(a):
    return a["WFWorkflowActionParameters"]["UUID"]


# DuckDuckGo: no input needed.
ddg_run = shell('exec "$EMASK" ddg 2>&1')
ddg = workflow([ddg_run, notify("DDG Masked Email", (uuid_of(ddg_run), "Shell Script Result"))], 59511, 4282601983)

# Fastmail: ask for the site, pass it on stdin (no quoting games).
ask = action(
    "is.workflow.actions.ask",
    {"WFAskActionPrompt": "Which site or service is this masked email for?", "WFInputType": "Text", "WFAllowsMultilineText": False},
)
fm_run = shell('site="$(cat)"\nexec "$EMASK" fm --for "$site" 2>&1', stdin_from=(uuid_of(ask), "Provided Input"))
fm = workflow([ask, fm_run, notify("Fastmail Masked Email", (uuid_of(fm_run), "Shell Script Result"))], 59511, 431817727)

for name, wf in (("DDG Masked Email", ddg), ("Fastmail Masked Email", fm)):
    path = os.path.join(OUT, f"{name}.shortcut")
    with open(path, "wb") as fh:
        plistlib.dump(wf, fh, fmt=plistlib.FMT_BINARY)
    print("wrote", path)

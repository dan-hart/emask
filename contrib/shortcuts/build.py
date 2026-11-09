#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Generate Apple Shortcuts that wrap emask.

    python3 contrib/shortcuts/build.py [OUTPUT_DIR] [--tailnet-url HTTPS_ORIGIN]

Produces two unsigned .shortcut plists:

  "DDG Masked Email"       runs `emask ddg`, shows a notification
  "Fastmail Masked Email"  asks which site the address is for, runs `emask fm --for <site>`

With --tailnet-url, also produces the iOS "New Masked Email" shortcut.

Then sign and open them (macOS 12+):

    shortcuts sign --mode anyone --input "DDG Masked Email.shortcut" --output "DDG Masked Email.signed.shortcut"
    open "DDG Masked Email.signed.shortcut"      # click "Add Shortcut"

Install and configure emask first:

    brew install dan-hart/tap/emask
    emask setup

Upgrade with: brew update && brew upgrade dan-hart/tap/emask
Requirements: emask on PATH, in /opt/homebrew/bin, /usr/local/bin, or ~/.cargo/bin, and
Shortcuts → Settings → Advanced → "Allow Running Scripts" enabled.
The shortcut's shell script is deliberately tiny; all logic lives in emask.
"""
import argparse
from pathlib import Path
import plistlib
from urllib.parse import urlsplit
import uuid

# Shortcuts runs scripts with a minimal PATH; find emask explicitly.
PRELUDE = (
    'EMASK="$(command -v emask || true)"\n'
    'if [ -z "$EMASK" ]; then\n'
    '  for candidate in /opt/homebrew/bin/emask /usr/local/bin/emask "$HOME/.cargo/bin/emask"; do\n'
    '    if [ -x "$candidate" ]; then EMASK="$candidate"; break; fi\n'
    '  done\n'
    'fi\n'
    '[ -n "$EMASK" ] || { echo "emask not found. Install with: brew install dan-hart/tap/emask; then run emask setup in a terminal." >&2; exit 1; }\n'
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


def iphone_workflow(endpoint):
    def post(provider):
        return action("is.workflow.actions.downloadurl", {
            "WFURL": endpoint,
            "WFHTTPMethod": "POST",
            "WFHTTPBodyType": "JSON",
            "WFJSONValues": {
                "WFSerializationType": "WFDictionaryFieldValue",
                "Value": {"WFDictionaryFieldValueItems": [{
                    "WFItemType": 0,
                    "WFKey": text(["provider"]),
                    "WFValue": text([provider]),
                }]},
            },
        })

    group = uid()
    conditional = "is.workflow.actions.conditional"
    begin = action(conditional, {
        "GroupingIdentifier": group,
        "WFControlFlowMode": 0,
        "WFCondition": 100,  # Has Any Value
        "WFInput": {"Type": "Variable", "Variable": {
            "Value": {"Type": "ExtensionInput"},
            "WFSerializationType": "WFTextTokenAttachment",
        }},
    })
    otherwise = action(conditional, {"GroupingIdentifier": group, "WFControlFlowMode": 1})
    end = action(conditional, {"GroupingIdentifier": group, "WFControlFlowMode": 2})
    email = action("is.workflow.actions.getvalueforkey", {
        "WFDictionaryKey": "email",
        "WFInput": attach(uuid_of(end), "If Result"),
    })
    copy = action("is.workflow.actions.setclipboard", {
        "WFInput": attach(uuid_of(email), "Dictionary Value"),
        "WFLocalOnly": True,
    })
    result = workflow([begin, post("ddg"), otherwise, post("default"), end, email, copy],
                      59511, 4282601983)
    result.update({
        "WFWorkflowTypes": ["ActionExtension"],
        "WFWorkflowInputContentItemClasses": ["WFSafariWebPageContentItem"],
        "WFWorkflowHasShortcutInputVariables": True,
        "WFWorkflowName": "New Masked Email",
    })
    return result


def tailnet_endpoint(value):
    try:
        url = urlsplit(value)
        port = url.port
    except ValueError as error:
        raise argparse.ArgumentTypeError("Expected an HTTPS Tailnet origin") from error
    if (url.scheme != "https" or not (url.hostname or "").endswith(".ts.net")
            or url.username is not None or url.path not in ("", "/") or url.query or url.fragment):
        raise argparse.ArgumentTypeError("Expected an HTTPS Tailnet origin, such as https://emask.example.ts.net")
    authority = url.hostname if port is None else f"{url.hostname}:{port}"
    return f"https://{authority}/v1/aliases"


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

parser = argparse.ArgumentParser(description="Generate Emask shortcuts")
parser.add_argument("output_dir", nargs="?", type=Path, default=Path("."))
parser.add_argument("--tailnet-url", type=tailnet_endpoint, metavar="HTTPS_ORIGIN")
args = parser.parse_args()
args.output_dir.mkdir(parents=True, exist_ok=True)

shortcuts = [("DDG Masked Email", ddg), ("Fastmail Masked Email", fm)]
if args.tailnet_url:
    shortcuts.append(("New Masked Email", iphone_workflow(args.tailnet_url)))

for name, wf in shortcuts:
    path = args.output_dir / f"{name}.shortcut"
    path.write_bytes(plistlib.dumps(wf, fmt=plistlib.FMT_BINARY))
    print("wrote", path)

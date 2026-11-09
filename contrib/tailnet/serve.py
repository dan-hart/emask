#!/usr/bin/env python3
# SPDX-License-Identifier: AGPL-3.0-or-later
"""Serve Emask on loopback behind Tailscale Serve."""

import argparse
from http.server import BaseHTTPRequestHandler, HTTPServer
import json
import shutil
import subprocess
import sys


class AliasHandler(BaseHTTPRequestHandler):
    # Bound the time spent waiting for a request from the proxy.
    timeout = 5

    def do_POST(self):
        if self.path != "/v1/aliases":
            return self.reply(404, error="Unknown endpoint")
        if self.headers.get_all("Tailscale-User-Login") != [self.server.allowed_login]:
            return self.reply(403, error="This Tailscale identity cannot use Emask")
        if self.headers.get_content_type() != "application/json":
            return self.reply(415, error="Send application/json")

        lengths = self.headers.get_all("Content-Length", [])
        if self.headers.get("Transfer-Encoding") or len(lengths) != 1 or not lengths[0].isdigit():
            return self.reply(400, error="A single Content-Length is required")
        try:
            length = int(lengths[0])
            if length > 1024:
                return self.reply(413, error="Request too large")
            body = json.loads(self.rfile.read(length))
        except (ValueError, OSError):
            return self.reply(400, error="Invalid JSON request")
        if not isinstance(body, dict) or body.keys() - {"provider"}:
            return self.reply(400, error="Send a JSON object with an optional provider")
        provider = body.get("provider", "default")
        if provider not in ("default", "ddg"):
            return self.reply(400, error="Provider must be default or ddg")

        command = [self.server.emask, "new", "--json", "--no-copy"]
        if provider == "ddg":
            command.append("ddg")
        try:
            result = subprocess.run(
                command, stdin=subprocess.DEVNULL, capture_output=True, check=True, timeout=45,
            )
            email = json.loads(result.stdout)["email"]
            if not isinstance(email, str) or not email.strip():
                raise ValueError("Missing address")
        except (subprocess.SubprocessError, OSError, ValueError, TypeError, KeyError) as error:
            # Provider responses and CLI diagnostics may contain private data.
            print(f"emask mint failed ({type(error).__name__})", file=sys.stderr, flush=True)
            return self.reply(502, error="Emask could not mint an address. Check the host provider configuration.")
        self.reply(201, email=email)

    def do_GET(self):
        self.reply(405, error="Use POST /v1/aliases")

    def reply(self, status, **payload):
        data = json.dumps(payload).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(data)))
        self.send_header("Cache-Control", "no-store")
        if status == 405:
            self.send_header("Allow", "POST")
        self.end_headers()
        self.wfile.write(data)

    def log_request(self, code="-", size="-"):
        # Keep URLs, login names, and addresses out of the request log.
        pass


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--allow-login", required=True, help="Allowed Tailscale user login")
    parser.add_argument("--port", type=int, default=8770, help="Loopback port (default: 8770)")
    parser.add_argument("--emask", default="emask", help="Path to the Emask executable")
    args = parser.parse_args()
    if not args.allow_login.strip():
        parser.error("--allow-login must be nonempty")
    emask = shutil.which(args.emask)
    if emask is None:
        parser.error(f"Emask executable not found: {args.emask}")
    # Serve strips client-supplied identity headers before forwarding them.
    # Keep this listener on loopback so remote callers must go through Serve.
    with HTTPServer(("127.0.0.1", args.port), AliasHandler) as server:
        server.allowed_login = args.allow_login
        server.emask = emask
        print(f"http://127.0.0.1:{server.server_port}", flush=True)
        try:
            server.serve_forever()
        except KeyboardInterrupt:
            pass


if __name__ == "__main__":
    main()

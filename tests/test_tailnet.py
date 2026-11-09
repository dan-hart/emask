# SPDX-License-Identifier: AGPL-3.0-or-later
import http.client
import json
import os
from pathlib import Path
import plistlib
import subprocess
import sys
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[1]
LOGIN = "phone-owner@example.com"


class TailnetAPI(unittest.TestCase):
    def setUp(self):
        directory = tempfile.TemporaryDirectory()
        self.addCleanup(directory.cleanup)
        self.calls = Path(directory.name) / "calls"
        binary = Path(directory.name) / "emask"
        binary.write_text(
            f"#!{sys.executable}\n"
            "import json, os, sys\n"
            "with open(os.environ['TEST_CALLS'], 'a') as calls:\n"
            "    print(json.dumps(sys.argv[1:]), file=calls)\n"
            "if os.environ.get('TEST_FAIL') == '1':\n"
            "    sys.exit('private provider error')\n"
            "print(json.dumps({'email': 'masked@example.com'}))\n"
        )
        binary.chmod(0o700)
        self.binary = binary

    def start(self, fail=False):
        process = subprocess.Popen(
            [sys.executable, str(ROOT / "contrib/tailnet/serve.py"),
             "--port", "0", "--allow-login", LOGIN, "--emask", str(self.binary)],
            env={**os.environ, "TEST_CALLS": str(self.calls), "TEST_FAIL": str(int(fail))},
            stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True,
        )
        self.addCleanup(process.stdout.close)
        self.addCleanup(process.stderr.close)
        self.addCleanup(process.wait, timeout=5)
        self.addCleanup(lambda: process.terminate() if process.poll() is None else None)
        address = process.stdout.readline().strip()
        if not address.startswith("http://127.0.0.1:"):
            self.fail(process.communicate(timeout=5)[1])
        self.port = int(address.rsplit(":", 1)[1])

    def request(self, body="{}", login=LOGIN, method="POST", path="/v1/aliases",
                content_type="application/json"):
        connection = http.client.HTTPConnection("127.0.0.1", self.port, timeout=5)
        try:
            headers = {"Content-Type": content_type}
            if login is not None:
                headers["Tailscale-User-Login"] = login
            connection.request(method, path, body, headers)
            response = connection.getresponse()
            return response.status, json.loads(response.read())
        finally:
            connection.close()

    def test_mints_with_default_or_duckduckgo(self):
        self.start()
        self.assertEqual(self.request(), (201, {"email": "masked@example.com"}))
        self.assertEqual(self.request('{"provider":"default"}')[0], 201)
        self.assertEqual(self.request('{"provider":"ddg"}')[0], 201)
        calls = [json.loads(line) for line in self.calls.read_text().splitlines()]
        self.assertEqual(calls, [
            ["new", "--json", "--no-copy"],
            ["new", "--json", "--no-copy"],
            ["new", "--json", "--no-copy", "ddg"],
        ])

    def test_rejects_untrusted_or_invalid_requests_before_minting(self):
        self.start()
        self.assertEqual(self.request(login=None)[0], 403)
        self.assertEqual(self.request(login="other@example.com")[0], 403)
        self.assertEqual(self.request(method="GET")[0], 405)
        self.assertEqual(self.request(path="/wrong")[0], 404)
        self.assertEqual(self.request(content_type="text/plain")[0], 415)
        self.assertEqual(self.request("x" * 1025)[0], 413)
        for body in ("[]", "{", '{"provider":"--help"}', '{"site":"https://example.com"}'):
            with self.subTest(body=body):
                self.assertEqual(self.request(body)[0], 400)
        self.assertFalse(self.calls.exists())

    def test_provider_failure_does_not_expose_cli_output(self):
        self.start(fail=True)
        status, body = self.request()
        self.assertEqual(status, 502)
        self.assertIn("error", body)
        self.assertNotIn("private provider error", str(body))


class IPhoneShortcut(unittest.TestCase):
    def test_generates_share_sheet_workflow_with_both_provider_paths(self):
        with tempfile.TemporaryDirectory() as directory:
            base = "https://emask.example.ts.net:8443"
            subprocess.run(
                [sys.executable, str(ROOT / "contrib/shortcuts/build.py"), directory,
                 "--tailnet-url", base], check=True, capture_output=True,
            )
            workflow = plistlib.loads((Path(directory) / "New Masked Email.shortcut").read_bytes())
        self.assertEqual(workflow["WFWorkflowTypes"], ["ActionExtension"])
        self.assertIn("WFSafariWebPageContentItem", workflow["WFWorkflowInputContentItemClasses"])
        actions = workflow["WFWorkflowActions"]
        self.assertEqual(actions[0]["WFWorkflowActionIdentifier"], "is.workflow.actions.conditional")
        self.assertEqual(actions[0]["WFWorkflowActionParameters"]["WFCondition"], 100)
        posts = [action["WFWorkflowActionParameters"] for action in actions
                 if action["WFWorkflowActionIdentifier"] == "is.workflow.actions.downloadurl"]
        self.assertEqual([post["WFHTTPMethod"] for post in posts], ["POST", "POST"])
        self.assertEqual([post["WFURL"] for post in posts], [base + "/v1/aliases"] * 2)
        self.assertEqual([post["WFJSONValues"]["Value"]["WFDictionaryFieldValueItems"][0]
                          ["WFValue"]["Value"]["string"] for post in posts], ["ddg", "default"])
        self.assertEqual(actions[-1]["WFWorkflowActionIdentifier"], "is.workflow.actions.setclipboard")

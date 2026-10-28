# SPDX-License-Identifier: AGPL-3.0-or-later
# Real-terminal setup checks with synthetic credentials; no network requests.
import errno
import os
from pathlib import Path
import pty
import select
import signal
import subprocess
import tempfile
import sys
import time

# Usage: python3 tests/terminal_smoke.py target/release/emask
BINARY = str(Path(sys.argv[1]).resolve())


class Session:
    def __init__(self, args):
        self.pid, self.fd = pty.fork()
        if self.pid == 0:
            os.execv(BINARY, [BINARY, *args])
        self.output = b''
        self.cursor = 0

    def read(self, timeout=0.2):
        if select.select([self.fd], [], [], timeout)[0]:
            try:
                data = os.read(self.fd, 8192)
            except OSError as error:
                if error.errno == errno.EIO:
                    return False
                raise
            self.output += data
            return bool(data)
        return True

    def answer(self, prompt, answer):
        deadline = time.monotonic() + 10
        needle = prompt.encode()
        while needle not in self.output[self.cursor:]:
            assert time.monotonic() < deadline, ('prompt timeout', prompt, self.output)
            assert self.read(), ('early exit', prompt, self.output)
        self.cursor = self.output.index(needle, self.cursor) + len(needle)
        # Let rpassword finish configuring terminal echo after printing its prompt.
        time.sleep(0.05)
        os.write(self.fd, answer.encode())

    def finish(self, expected=0):
        deadline = time.monotonic() + 10
        while True:
            self.read()
            pid, status = os.waitpid(self.pid, os.WNOHANG)
            if pid:
                while self.read(0):
                    pass
                os.close(self.fd)
                assert os.waitstatus_to_exitcode(status) == expected, self.output
                return self.output.decode()
            if time.monotonic() >= deadline:
                os.kill(self.pid, signal.SIGKILL)
                os.waitpid(self.pid, 0)
                raise AssertionError(('exit timeout', self.output))


with tempfile.TemporaryDirectory(prefix='emask-pty-') as directory:
    config = Path(directory) / 'private' / 'config.toml'
    session = Session(['--config', str(config), 'setup'])
    session.answer('Set up this service? [Y/n]: ', 'n\n')
    session.answer('Set up this service? [Y/n]: ', 'n\n')
    output = session.finish()
    assert 'No enabled services' in output
    assert not config.exists()
    print('PASS: all-skipped setup creates no config')

    session = Session(['--config', str(config), 'setup', 'ddg'])
    session.answer('Set up this service? [Y/n]: ', '\n')
    session.answer('login [1]: ', '\n')
    session.answer('Paste token (hidden; Enter to skip): ', 'synthetic-pty-token\n')
    output = session.finish()
    assert 'synthetic-pty-token' not in output, 'token was echoed'
    assert 'not verified' in output
    assert 'Setup complete' in output
    assert f"emask --config '{config}'" in output
    assert 'synthetic-pty-token' in config.read_text()
    assert config.stat().st_mode & 0o777 == 0o600
    assert config.parent.stat().st_mode & 0o777 == 0o700
    print('PASS: real-terminal token entry is hidden, saved privately, and gives a usable command')

    before = config.read_bytes()
    session = Session(['--config', str(config), 'setup', 'ddg'])
    session.answer('Replace them? [y/N]: ', '\n')
    output = session.finish()
    assert 'Kept saved credentials' in output
    assert config.read_bytes() == before
    print('PASS: rerun keeps saved credentials by default')

    session = Session(['--config', str(config), 'setup', 'fm'])
    session.answer('Set up this service? [Y/n]: ', '\x04')
    output = session.finish(expected=1)
    assert 'input ended' in output
    assert config.read_bytes() == before
    print('PASS: EOF cancels without changing saved credentials')

    result = subprocess.run([BINARY, '--config', str(config), 'setup'], input='', capture_output=True, text=True)
    assert result.returncode == 1
    assert 'requires an interactive terminal' in result.stderr
    assert config.read_bytes() == before
    print('PASS: redirected wizard input is rejected with scripting guidance')

manifest = Path(__file__).resolve().parent.parent / 'Cargo.toml'
version = next(line.split(chr(34))[1] for line in manifest.read_text().splitlines() if line.startswith('version = '))
assert subprocess.check_output([BINARY, '--version'], text=True).strip() == f'emask {version}'
print(f'PASS: release binary reports version {version}')

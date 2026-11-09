#!/usr/bin/env python3
"""Start Chord's project-owned backend if needed, then open its Rust client."""
import fcntl
import json
import os
from pathlib import Path
import socket
import subprocess
import sys
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
PRIVATE = ROOT / 'private'


def configuration():
    try:
        return json.loads((PRIVATE / 'config.json').read_text())
    except FileNotFoundError:
        return {}


def healthy(kind):
    url = ('http://127.0.0.1:4808/_matrix/client/versions' if kind == 'runtime'
           else 'http://127.0.0.1:4789/api/status')
    headers = {}
    if kind == 'api':
        key = configuration().get('apiKey')
        if not key:
            return False
        headers['Authorization'] = 'Bearer ' + key
    try:
        request = urllib.request.Request(url, headers=headers)
        with urllib.request.urlopen(request, timeout=2) as response:
            data = json.load(response)
        return bool(data.get('versions')) if kind == 'runtime' else data.get('independent') is True
    except (OSError, ValueError):
        return False


def occupied(port):
    try:
        with socket.create_connection(('127.0.0.1', port), timeout=.5):
            return True
    except OSError:
        return False


def start(kind, command):
    port = 4808 if kind == 'runtime' else 4789
    if healthy(kind):
        return
    # A concurrent launch may still be coming up. Never open another process
    # against the same databases merely because its health check has not passed.
    if occupied(port):
        for _ in range(15):
            if healthy(kind):
                return
            time.sleep(1)
        raise RuntimeError(f'Port {port} is occupied but Chord is not ready. See private/desktop-{kind}.log.')
    print('Starting local chat engine…' if kind == 'runtime' else 'Starting local chat adapter…', flush=True)
    with (PRIVATE / f'desktop-{kind}.log').open('ab') as log:
        child = subprocess.Popen(command, cwd=ROOT, stdin=subprocess.DEVNULL,
                                 stdout=log, stderr=log, start_new_session=True)
    deadline = time.monotonic() + 900
    while time.monotonic() < deadline:
        if healthy(kind):
            return
        if child.poll() is not None:
            raise RuntimeError(f'Chord {kind} did not start. See private/desktop-{kind}.log.')
        time.sleep(1)
    raise RuntimeError(f'Chord {kind} is still starting. See private/desktop-{kind}.log.')


def start_backend():
    with (PRIVATE / 'desktop-start.lock').open('a') as lock:
        try:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        except BlockingIOError:
            return
        start('runtime', ['/run/current-system/sw/bin/nix-shell', str(ROOT / 'runtime.nix'),
                          '--run', 'exec node scripts/runtime.mjs'])
        start('api', ['node', str(ROOT / 'server.mjs')])


def focus_window():
    try:
        clients = json.loads(subprocess.check_output(['hyprctl', 'clients', '-j'], timeout=2))
        window = next((c for c in clients if c.get('class') == 'dev.chord.Chord'), None)
        if window:
            subprocess.run(['hyprctl', 'dispatch', 'focuswindow',
                            'address:' + window['address']], check=False)
    except (OSError, ValueError, subprocess.SubprocessError):
        pass


def main():
    os.umask(0o077)
    PRIVATE.mkdir(mode=0o700, exist_ok=True)
    os.chdir(ROOT)
    if '--backend-only' in sys.argv:
        start_backend()
        return
    # Keep one client per desktop session, including during slow cold starts.
    client_lock = (PRIVATE / 'desktop-window.lock').open('a')
    try:
        fcntl.flock(client_lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        focus_window()
        return
    if not (PRIVATE / 'config.json').is_file():
        start_backend()
    if not healthy('runtime') or not healthy('api'):
        with (PRIVATE / 'desktop-client.log').open('ab') as log:
            subprocess.Popen([sys.executable, __file__, '--backend-only'], cwd=ROOT,
                             stdin=subprocess.DEVNULL, stdout=log, stderr=log,
                             start_new_session=True)
    binary = ROOT / 'target/release/chord'
    if not binary.is_file():
        binary = ROOT / 'target/debug/chord'
    if not binary.is_file():
        print('Building the Chord client…', flush=True)
        subprocess.run(['/run/current-system/sw/bin/nix-shell', str(ROOT / 'shell.nix'),
                        '--run', 'cargo build --locked'], cwd=ROOT, check=True)
    # Existing installations reconnect through the client's normal refresh.
    os.set_inheritable(client_lock.fileno(), True)
    os.execv(str(binary), [str(binary)])


if __name__ == '__main__':
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as error:
        print(f'Chord could not open: {error}', file=sys.stderr)
        if sys.stdin.isatty():
            input('Press Enter to close. ')
        raise SystemExit(1)

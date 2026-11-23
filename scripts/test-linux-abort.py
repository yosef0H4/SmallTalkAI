#!/usr/bin/env python3
"""Exercise repeated Abort through a persistent Linux daemon on private X11 and D-Bus."""

import argparse
import json
import os
from pathlib import Path
import signal
import socketserver
import struct
import subprocess
import sys
import tempfile
import threading
import time
from contextlib import ExitStack, suppress


def wait_for(probe, description, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        if result := probe():
            return result
        time.sleep(0.025)
    raise AssertionError(description)


def child_pids(parent):
    children = set()
    for task in Path(f"/proc/{parent}/task").glob("*/children"):
        try:
            children.update(task.read_text().split())
        except FileNotFoundError:
            continue
    return children


def zombie_children(parent):
    zombies = []
    for pid in child_pids(parent):
        try:
            stat = Path(f"/proc/{pid}/stat").read_text()
        except FileNotFoundError:
            continue
        if stat.rsplit(") ", 1)[1].startswith("Z "):
            zombies.append(pid)
    return zombies


def exercise(bins, root):
    class Capture(socketserver.StreamRequestHandler):
        def emit(self, item):
            self.wfile.write(json.dumps(item).encode() + b"\n")

        def handle(self):
            hello = json.loads(self.rfile.readline())
            assert hello["token"] == token
            self.emit({"type": "hello_ack", "protocol": 3, "service_pid": os.getpid()})
            while line := self.rfile.readline():
                request = json.loads(line)
                name = request["command"]["name"]
                if name == "cancel":
                    recording.clear()
                assert name in {"ping", "cancel", "show_notice", "shutdown", "refresh_shortcuts"}, name
                self.emit({"type": "response", "request_id": request["request_id"],
                           "response": {"ok": True, "message": "", "events": [], "values": {}}})

    class Reader(socketserver.StreamRequestHandler):
        def handle(self):
            header = self.rfile.read(4)
            if not header:
                return
            size = struct.unpack("<I", header)[0]
            assert self.rfile.read(1) == b"\0"
            request = json.loads(self.rfile.read(size - 1))
            if request == {"op": "action", "action": "stop"}:
                stop_received.set()
                assert release_stop.wait(12), "slow reader fixture was never released"
                reading.clear()
            else:
                assert request["op"] in {"status", "shutdown"}, request
            reply = json.dumps({"ok": True, "message": "Stopped",
                                "state": {"state": "playing" if reading.is_set() else "idle"}}).encode()
            self.wfile.write(struct.pack("<I", len(reply) + 1) + b"\0" + reply)

    env = dict(os.environ, SMALLTALKAI_BINDIR=str(root / "bin"), XDG_DATA_HOME=str(root / "data"),
               XDG_CONFIG_HOME=str(root / "config"), SMALLTALKAI_RUNTIME_ROOT=str(root / "runtime"),
               SMALLTALKAI_READER_DATA=str(root / "reader"), XDG_SESSION_TYPE="x11",
               XDG_RUNTIME_DIR=str(root / "xdg"), XDG_CURRENT_DESKTOP="Test",
               SMALLTALK_TEST_CAPTURE_PID=str(root / "capture.pid"))
    env.pop("WAYLAND_DISPLAY", None)
    (root / "bin").mkdir()
    (root / "reader").mkdir()
    (root / "xdg").mkdir(mode=0o700)
    capture_stub = root / "bin/smalltalkai-capture"
    capture_stub.write_text('#!/bin/sh\nprintf "%s\\n" "$$" > "$SMALLTALK_TEST_CAPTURE_PID"\nexec sleep 60\n')
    capture_stub.chmod(0o700)
    config = root / "settings.json"
    config.write_text(json.dumps({"general": {"linux_hotkey_backend": "x11", "cancel_hotkey": "Meta+Ctrl+Z"}}))
    recording, reading, stop_received, release_stop = (threading.Event() for _ in range(4))
    bus = ["busctl", "--user", "--timeout=2s"]

    def get_abort_menu(daemon_pid):
        listing = subprocess.check_output([*bus, "--no-legend", "list"], text=True)
        for row in listing.splitlines():
            name, pid, *_ = row.split()
            if not (name.startswith(":") and pid == str(daemon_pid)):
                continue
            layout = subprocess.run([*bus, "--json=short", "--", "call", name, "/MenuBar",
                                     "com.canonical.dbusmenu", "GetLayout", "iias", "0", "-1", "0"],
                                    capture_output=True, text=True)
            if layout.returncode != 0:
                continue
            _, (_, _, children) = json.loads(layout.stdout)["data"]
            for child in children:
                item, props, _ = child["data"]
                if props.get("label", {}).get("data") == "Abort":
                    return name, str(item)
        return None

    with ExitStack() as stack:
        capture = stack.enter_context(socketserver.ThreadingTCPServer(("127.0.0.1", 0), Capture))
        reader_socket = root / "reader/smalltalkai-reader.sock"
        reader = stack.enter_context(socketserver.ThreadingUnixStreamServer(str(reader_socket), Reader))
        for listener in (capture, reader):
            threading.Thread(target=listener.serve_forever, daemon=True).start()
            stack.callback(listener.shutdown)
        with (root / "daemon.log").open("w") as log:
            daemon = subprocess.Popen([bins / "smalltalkai-linux", "daemon", "--config", config],
                                      env=env, cwd=root, stdout=log, stderr=log, start_new_session=True)
        try:
            token_path = wait_for(lambda: next((root / "data").rglob("linux-token"), None),
                                  "persistent daemon did not create a capture token")
            token = token_path.read_text().strip()
            state_dir = token_path.parent / "state"
            state_dir.mkdir(exist_ok=True)
            (state_dir / "linux-capture-state.json").write_text(json.dumps({
                "protocol": 3, "pid": os.getpid(), "started_unix_ms": 0,
                "address": f"127.0.0.1:{capture.server_address[1]}",
            }))
            backend = token_path.parent / "linux-hotkey-backend.json"
            wait_for(lambda: backend.exists() and json.loads(backend.read_text()).get("status") == "active",
                     "X11 Cancel shortcut did not register")
            name, item = wait_for(lambda: get_abort_menu(daemon.pid), "tray Abort was not registered")

            def abort(index):
                if index % 2 == 0:
                    subprocess.run([*bus, "call", name, "/MenuBar", "com.canonical.dbusmenu",
                                    "Event", "isvu", item, "clicked", "s", "", "0"], check=True)
                else:
                    # Separate shortcut presses from the daemon's 350 ms duplicate-event window.
                    time.sleep(0.4)
                    subprocess.run(["xdotool", "key", "Super_L+Control_L+z"], check=True)

            def exercise_abort(index, reader_present):
                session = token_path.parent / "linux-recording-session.json"
                session.write_text(json.dumps({"recording": True, "session_id": 1, "updated_at": 0}))
                recording.set()
                stop_received.clear()
                if reader_present:
                    reading.set()
                if index == 0:
                    release_stop.clear()
                abort(index)
                wait_for(lambda: not recording.is_set(), "dictation waited for the Reader or did not cancel")
                assert not json.loads(session.read_text())["recording"], "pending delivery was not invalidated"
                if reader_present:
                    wait_for(stop_received.is_set, "Reader did not receive Stop")
                    if index == 0:
                        assert reading.is_set(), "slow Reader was released before dictation cancelled"
                    release_stop.set()
                    wait_for(lambda: not reading.is_set(), "Reader continued playing")

            for index in range(5):
                exercise_abort(index, reader_present=True)
            reader.shutdown()
            reader_socket.unlink()
            exercise_abort(5, reader_present=False)
            wait_for(lambda: (root / "daemon.log").read_text().count("] cancelled") == 6,
                     "daemon did not finish all Abort actions")
            capture_pid = (root / "capture.pid").read_text().strip()
            try:
                wait_for(lambda: child_pids(daemon.pid) == {capture_pid},
                         "Abort helpers did not exit and get reaped")
            except AssertionError as error:
                raise AssertionError(f"{error}. Zombie children: {zombie_children(daemon.pid)}") from None
            assert not reader_socket.exists(), "Abort started an absent Reader"
            assert daemon.poll() is None, "daemon exited before zombie inspection"
            print("PASS repeated tray and shortcut Abort stopped dictation and Reader with zero zombie children")
        except Exception:
            print((root / "daemon.log").read_text(), file=sys.stderr)
            raise
        finally:
            release_stop.set()
            # Let the daemon collect its capture child before shutting down.
            if (root / "capture.pid").exists():
                with suppress(ProcessLookupError):
                    os.kill(int((root / "capture.pid").read_text()), signal.SIGTERM)
            try:
                daemon.wait(timeout=3)
            except subprocess.TimeoutExpired:
                os.killpg(daemon.pid, signal.SIGKILL)
                daemon.wait(timeout=3)


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--bin-dir", type=Path, default=Path("target/debug"))
    parser.add_argument("--private", action="store_true", help=argparse.SUPPRESS)
    args = parser.parse_args()
    if args.private:
        with tempfile.TemporaryDirectory(prefix="smalltalkai-abort-", dir="/tmp") as directory:
            exercise(args.bin_dir.resolve(), Path(directory))
    else:
        subprocess.run(["dbus-run-session", "--", "xvfb-run", "-a",
                        sys.executable, __file__, "--private", "--bin-dir", str(args.bin_dir)], check=True)

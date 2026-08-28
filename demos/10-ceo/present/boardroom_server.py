#!/usr/bin/env python3
"""Serve the animated boardroom AND capture new sessions from the UI.

Serves demos/10-ceo/present/ statically, plus a small API the page's Capture
button calls to (re)record a session live against the deployed CEO, save it as the
canonical recording, and rebuild boardroom.html — then the page reloads to play it.

    demos/10-ceo/present/boardroom_server.py            # http://localhost:8520/boardroom.html
    demos/10-ceo/present/boardroom_server.py 8531

Endpoints:
    POST /api/capture   {"session":"meeting"|"debate"}   -> starts a capture
    GET  /api/capture/status                              -> {running,session,beats,total,phase,ok,message}

The capture drives demos/10-ceo/{drive,debate}.py against $CEO_API_URL
(default http://localhost:8099 — port-forward svc/ceo first).
"""
from __future__ import annotations
import json
import os
import subprocess
import sys
import threading
import time
from http.server import SimpleHTTPRequestHandler, ThreadingHTTPServer

HERE = os.path.dirname(os.path.abspath(__file__))
REPO = os.path.abspath(os.path.join(HERE, "..", "..", ".."))
sys.path.insert(0, HERE)
import state  # noqa: E402  (read_jsonl / save_recording)

DRIVER = {"meeting": ("demos/10-ceo/drive.py", 6),
          "debate":  ("demos/10-ceo/debate.py", 5)}
CEO_URL = os.environ.get("CEO_API_URL", "http://localhost:8099")

_LOCK = threading.Lock()
CAP = {"running": False, "session": None, "beats": 0, "total": 0,
       "phase": "idle", "ok": None, "message": ""}


def _set(**kw):
    with _LOCK:
        CAP.update(kw)


def _rebuild():
    subprocess.run([sys.executable, os.path.join(HERE, "build_boardroom.py")],
                   cwd=REPO, check=False)


def _capture_worker(session: str):
    rel, total = DRIVER[session]
    _set(running=True, session=session, beats=0, total=total,
         phase="capturing", ok=None, message="Convening the board…")
    jsonl = os.path.join("/tmp", f"boardroom-{session}.jsonl")
    open(jsonl, "w").close()
    env = dict(os.environ, CEO_API_URL=CEO_URL, PYTHONPATH=REPO,
               XDG_RUNTIME_DIR=os.environ.get("XDG_RUNTIME_DIR", "/run/user/1000"),
               XDG_DATA_HOME=os.environ.get("XDG_DATA_HOME", os.path.expanduser("~/.local/share")))
    proc = subprocess.Popen([sys.executable, rel, "--emit-jsonl", jsonl],
                            cwd=REPO, env=env,
                            stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    try:
        while proc.poll() is None:
            try:
                n = sum(1 for ln in open(jsonl) if ln.strip())
            except OSError:
                n = 0
            _set(beats=n, message=f"Officer {min(n + 1, total)} of {total} taking the floor…")
            time.sleep(1.0)
        beats = state.read_jsonl(open(jsonl).read())
        if len(beats) >= total:
            d = os.path.join(HERE, "recordings", session)
            os.makedirs(d, exist_ok=True)
            p = state.save_recording(d, beats)
            import shutil
            shutil.copy(p, os.path.join(d, "canonical.json"))
            _rebuild()
            _set(beats=len(beats), phase="done", ok=True,
                 message=f"Captured {len(beats)} beats — reloading.")
        else:
            _set(phase="failed", ok=False,
                 message=f"Capture incomplete ({len(beats)}/{total}) — kept the previous recording.")
    except Exception as e:  # noqa: BLE001
        _set(phase="failed", ok=False, message=f"Capture error: {e}")
    finally:
        _set(running=False)


class Handler(SimpleHTTPRequestHandler):
    def __init__(self, *a, **kw):
        super().__init__(*a, directory=HERE, **kw)

    def log_message(self, *a):  # quiet
        pass

    def _json(self, code, obj):
        body = json.dumps(obj).encode()
        self.send_response(code)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def do_GET(self):
        if self.path.startswith("/api/capture/status"):
            with _LOCK:
                return self._json(200, dict(CAP))
        return super().do_GET()

    def do_POST(self):
        if self.path.startswith("/api/capture"):
            n = int(self.headers.get("Content-Length", 0) or 0)
            try:
                req = json.loads(self.rfile.read(n) or b"{}")
            except json.JSONDecodeError:
                req = {}
            session = req.get("session", "meeting")
            if session not in DRIVER:
                return self._json(400, {"error": "unknown session"})
            with _LOCK:
                if CAP["running"]:
                    return self._json(409, {"error": "a capture is already running"})
            threading.Thread(target=_capture_worker, args=(session,), daemon=True).start()
            return self._json(202, {"started": True, "session": session})
        self._json(404, {"error": "not found"})


def main():
    port = int(sys.argv[1]) if len(sys.argv) > 1 else 8520
    _rebuild()
    httpd = ThreadingHTTPServer(("0.0.0.0", port), Handler)
    print(f"▶ boardroom: http://localhost:{port}/boardroom.html   (CEO at {CEO_URL})")
    httpd.serve_forever()


if __name__ == "__main__":
    main()

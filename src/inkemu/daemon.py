import json
import os
import queue
import threading
import time
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any, Callable

from .clock import parse_duration
from .emu import KEYS, SWIPES, Emu

DEFAULT_PORT = 7480
HOST = "127.0.0.1"
REPLY_TIMEOUT_S = 600
LOG_TAIL = 20


class UsageError(Exception):
    pass


def _int(v: Any) -> int:
    if isinstance(v, bool):
        raise UsageError(f"expected an integer, got {v!r}")
    try:
        return int(v)
    except (TypeError, ValueError):
        raise UsageError(f"expected an integer, got {v!r}") from None


def _str(v: Any) -> str:
    return v if isinstance(v, str) else str(v)


def _choice(options: tuple[str, ...]) -> Callable[[Any], str]:
    def conv(v: Any) -> str:
        if v not in options:
            raise UsageError(f"must be one of {', '.join(options)}, got {v!r}")
        return v
    return conv


def _yes_no(v: Any) -> bool:
    if isinstance(v, bool):
        return v
    if v in ("yes", "no"):
        return v == "yes"
    raise UsageError("answer takes yes or no")


def _duration(v: Any) -> str:
    s = str(v)
    try:
        parse_duration(s)
    except ValueError as e:
        raise UsageError(str(e)) from None
    return s


# verb -> (usage, [(arg name, converter, required)], handler(emu, args))
def _spec() -> dict[str, tuple[str, list[tuple[str, Callable, bool]], Callable[[Emu, dict], None]]]:
    xy = [("x", _int, True), ("y", _int, True)]
    nothing: list = []
    return {
        "tap": ("tap X Y", xy, lambda e, a: e.tap(a["x"], a["y"])),
        "tap-text": ("tap-text TEXT", [("text", _str, True)], lambda e, a: e.tap_text(a["text"])),
        "double-tap": ("double-tap X Y", xy, lambda e, a: e.double_tap(a["x"], a["y"])),
        "swipe": ("swipe DIR", [("direction", _choice(tuple(SWIPES)), True)], lambda e, a: e.swipe(a["direction"])),
        "hold": ("hold X Y [MS]", xy + [("ms", _int, False)], lambda e, a: e.hold(a["x"], a["y"], a.get("ms", 800))),
        "key": ("key KEY", [("key", _choice(KEYS), True)], lambda e, a: e.key(a["key"])),
        "type": ("type TEXT", [("text", _str, True)], lambda e, a: e.type_text(a["text"])),
        "answer": ("answer yes|no", [("answer", _yes_no, True)], lambda e, a: e.answer(a["answer"])),
        "wait": ("wait DURATION", [("duration", _duration, True)], lambda e, a: e.wait(a["duration"])),
        "lock": ("lock", nothing, lambda e, a: e.lock()),
        "unlock": ("unlock", nothing, lambda e, a: e.unlock()),
        "reload": ("reload", nothing, lambda e, a: e.reload()),
        "restart": ("restart", nothing, lambda e, a: e.restart()),
        "shot": ("shot [PATH]", [("path", _str, False)], lambda e, a: None),
        "texts": ("texts", nothing, lambda e, a: None),
        "log": ("log", nothing, lambda e, a: None),
        "state": ("state", nothing, lambda e, a: None),
        "resume": ("resume", nothing, lambda e, a: e.resume()),
        "stop": ("stop", nothing, lambda e, a: None),
    }


SPEC = _spec()
VERBS = tuple(usage for usage, _, _ in SPEC.values())


def _unknown(cmd: str) -> UsageError:
    return UsageError(f"unknown command {cmd!r}; verbs: " + "; ".join(VERBS))


def normalize(cmd: str, args: dict) -> dict:
    """Validate and convert args of one command; raises UsageError."""
    if cmd not in SPEC:
        raise _unknown(cmd)
    _, fields, _ = SPEC[cmd]
    known = {name for name, _, _ in fields}
    extra = set(args) - known - {"path"} if cmd == "shot" else set(args) - known
    if extra:
        raise UsageError(f"{cmd!r}: unexpected argument(s) {', '.join(sorted(extra))}")
    out = {}
    for name, conv, required in fields:
        if args.get(name) is None:
            if required:
                raise UsageError(f"{cmd!r}: missing argument {name!r} (usage: {SPEC[cmd][0]})")
            continue
        out[name] = conv(args[name])
    return out


def parse_words(words: list[str]) -> tuple[str, dict]:
    """`["tap", "1", "2"]` -> ("tap", {"x": 1, "y": 2}); raises UsageError on unknown verb, arity or bad values."""
    if not words:
        raise UsageError("empty command")
    cmd, rest = words[0], words[1:]
    if cmd not in SPEC:
        raise _unknown(cmd)
    usage, fields, _ = SPEC[cmd]
    lo = sum(1 for _, _, req in fields if req)
    if not lo <= len(rest) <= len(fields):
        want = str(lo) if lo == len(fields) else f"{lo}-{len(fields)}"
        raise UsageError(f"{cmd!r} takes {want} argument(s), got {len(rest)} (usage: {usage})")
    args = {name: w for (name, _, _), w in zip(fields, rest)}
    return cmd, normalize(cmd, args)


def _next_shot(shots: Path) -> Path:
    nums = [int(p.stem) for p in shots.glob("*.png") if p.stem.isdigit()] if shots.is_dir() else []
    return shots / f"{max(nums, default=0) + 1:03d}.png"


def execute(emu: Emu, cmd: str, args: dict | None = None) -> dict:
    """The single command table: run one command against `emu` and describe the result. Never raises."""
    args = args or {}
    error: str | None = None
    shot: str | None = None
    extra: dict = {}
    try:
        norm = normalize(cmd, args)
        if cmd != "resume":
            emu.paused = True
        SPEC[cmd][2](emu, norm)
        if cmd == "shot":
            shot = str(emu.save_shot(norm.get("path") or _next_shot(emu.state_dir / "shots")))
        elif cmd == "log":
            extra["log"] = list(emu.log)
    except UsageError as e:
        error = str(e)
    except (LookupError, RuntimeError, ValueError, OSError) as e:
        error = str(e)
    except Exception as e:
        error = f"{type(e).__name__}: {e}"
    try:
        texts = emu.texts()
    except Exception:
        texts = []
    return {"ok": error is None, "error": error, "texts": texts, "log_tail": emu.log[-LOG_TAIL:],
            "errors": list(emu.errors), "warnings": list(emu.warnings), "status": emu.status(), "shot": shot,
            **extra}


# ---- daemon.json ------------------------------------------------------------------------------

def state_root() -> Path:
    return Path(os.environ.get("INKEMU_STATE_ROOT", ".emu")).absolute()


def _alive(pid: int) -> bool:
    try:
        os.kill(pid, 0)
    except ProcessLookupError:
        return False
    except PermissionError:
        return True
    except OSError:
        return False
    return True


def read_info(root: Path | None = None) -> dict | None:
    """Contents of `<root>/daemon.json` (port, pid, app) if its process is alive, else None."""
    path = Path(root if root is not None else state_root()) / "daemon.json"
    try:
        info = json.loads(path.read_text(encoding="utf-8"))
        port, pid = int(info["port"]), int(info["pid"])
    except (OSError, ValueError, KeyError, TypeError):
        return None
    return info if _alive(pid) and port else None


def write_info(root: Path, port: int, app: str) -> None:
    root = Path(root)
    root.mkdir(parents=True, exist_ok=True)
    (root / "daemon.json").write_text(json.dumps({"port": port, "pid": os.getpid(), "app": app}), encoding="utf-8")


def remove_info(root: Path) -> None:
    info = Path(root) / "daemon.json"
    try:
        if json.loads(info.read_text(encoding="utf-8")).get("pid") == os.getpid():
            info.unlink()
    except (OSError, ValueError, AttributeError):
        pass


# ---- HTTP server ------------------------------------------------------------------------------

class _Server(ThreadingHTTPServer):
    daemon_threads = True
    allow_reuse_address = False


class Daemon:
    """HTTP front of one Emu. HTTP threads only enqueue; `pump()` runs commands on the caller's thread."""

    def __init__(self, emu: Emu, port: int = DEFAULT_PORT):
        self.emu = emu
        self.queue: queue.Queue = queue.Queue()
        self.stopped = False
        daemon = self

        class Handler(BaseHTTPRequestHandler):
            def log_message(self, *_):
                pass

            def _reply(self, code: int, body: dict) -> None:
                data = json.dumps(body, ensure_ascii=False).encode("utf-8")
                self.send_response(code)
                self.send_header("Content-Type", "application/json; charset=utf-8")
                self.send_header("Content-Length", str(len(data)))
                self.end_headers()
                self.wfile.write(data)

            def _forbidden(self) -> str | None:
                # a web page can POST to 127.0.0.1 (no-cors text/plain, DNS rebinding); our clients send neither Origin nor a foreign Host
                if self.headers.get("Host") not in (f"127.0.0.1:{daemon.port}", f"localhost:{daemon.port}"):
                    return "bad Host header"
                if "Origin" in self.headers:
                    return "cross-origin requests are not allowed"
                if self.headers.get("Content-Type", "").split(";")[0].strip().lower() != "application/json":
                    return "Content-Type must be application/json"
                return None

            def _other_method(self):
                self._reply(405, {"ok": False, "error": "POST /cmd only"})

            do_GET = do_PUT = do_DELETE = do_PATCH = do_HEAD = do_OPTIONS = _other_method

            def do_POST(self):
                if (why := self._forbidden()) is not None:
                    return self._reply(403, {"ok": False, "error": why})
                if self.path != "/cmd":
                    return self._reply(404, {"ok": False, "error": "POST /cmd only"})
                try:
                    raw = self.rfile.read(int(self.headers.get("Content-Length") or 0))
                    req = json.loads(raw)
                    cmd, args = req["cmd"], req.get("args") or {}
                    if not isinstance(cmd, str) or not isinstance(args, dict):
                        raise TypeError("cmd must be a string and args an object")
                except (ValueError, KeyError, TypeError) as e:
                    return self._reply(400, {"ok": False, "error": f"bad request: {e}"})
                reply: queue.Queue = queue.Queue(1)
                daemon.queue.put((cmd, args, reply))
                try:
                    self._reply(200, reply.get(timeout=REPLY_TIMEOUT_S))
                except queue.Empty:
                    self._reply(504, {"ok": False, "error": "the emulator did not answer in time"})

        try:
            self._server = _Server((HOST, port), Handler)
        except OSError as e:
            raise OSError(f"cannot listen on {HOST}:{port}: {e.strerror or e}"
                          " (is another inkemu daemon running? try --port)") from e
        self.port = self._server.server_address[1]
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True, name="inkemu-http")
        self._thread.start()

    def pump(self) -> int:
        """Run every queued command on this thread; returns how many ran."""
        n = 0
        while True:
            try:
                cmd, args, reply = self.queue.get_nowait()
            except queue.Empty:
                return n
            n += 1
            result = execute(self.emu, cmd, args)
            if cmd == "stop" and result["ok"]:
                self.stopped = True
            reply.put(result)

    def close(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        while True:
            try:
                _, _, reply = self.queue.get_nowait()
            except queue.Empty:
                break
            reply.put({"ok": False, "error": "the daemon is stopping"})


def serve(emu: Emu, port: int = DEFAULT_PORT, window: bool = False) -> None:
    """Run the daemon (and the Tk window if `window`) until `stop`, Ctrl-C or the window is closed."""
    root = state_root()
    d = Daemon(emu, port)
    write_info(root, d.port, emu.app_id)
    try:
        if window:
            from .window import run_window
            run_window(emu, d)
        else:
            try:
                while not d.stopped:
                    d.pump()
                    time.sleep(0.02)
            except KeyboardInterrupt:
                pass
    finally:
        try:
            emu.leave()
        finally:
            remove_info(root)
            d.close()

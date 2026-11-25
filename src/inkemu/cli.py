import argparse
import signal
import sys
import threading
from collections.abc import Callable

from . import _core
from .daemon import DEFAULT_PORT, WindowUnavailable, read_info, serve
from .emu import NET_MODES, Emu


def _window_parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(
        prog="inkemu window", description="open the emulator window and the command daemon", allow_abbrev=False
    )
    p.add_argument("app", help="app directory with manifest.json")
    p.add_argument("--net", choices=NET_MODES, default="replay")
    p.add_argument("--fresh", action="store_true", help="start from an empty state")
    p.add_argument("--time", help="virtual start time, ISO 8601")
    p.add_argument("--state", help="state directory (default: .emu/<app_id>)")
    p.add_argument("--fixtures", help="fixtures directory (default: tests/fixtures/<app_id>)")
    p.add_argument("--port", type=int, default=DEFAULT_PORT, help="daemon port")
    return p


def _window(argv: list[str]) -> int:
    try:
        args = _window_parser().parse_args(argv)
    except SystemExit as e:
        return e.code if isinstance(e.code, int) else 2
    if live := read_info():
        print(
            f"inkemu: a daemon is already running (pid {live['pid']}, port {live['port']}, app {live['app']})",
            file=sys.stderr,
        )
        return 2
    try:
        emu = Emu(
            args.app,
            state_dir=args.state,
            fixtures_dir=args.fixtures,
            net=args.net,
            fresh=args.fresh,
            start_time=args.time,
        )
    except ValueError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    try:
        serve(emu, args.port)
    except OSError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    except WindowUnavailable as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 1
    return 1 if emu.errors else 0


def _native(fn: Callable[..., int], *args) -> int:
    """Runs `fn` in Rust with the default SIGINT action: Python's handler only sets a flag Rust never checks."""
    # Rust writes to fd 1/2 directly
    sys.stdout.flush()
    sys.stderr.flush()
    if threading.current_thread() is not threading.main_thread():
        return fn(*args)
    previous = signal.signal(signal.SIGINT, signal.SIG_DFL)
    try:
        return fn(*args)
    finally:
        # None: the old handler was not installed from Python and cannot be put back
        if previous is not None:
            signal.signal(signal.SIGINT, previous)


def main(argv: list[str] | None = None) -> int:
    argv = sys.argv[1:] if argv is None else argv
    if argv[:1] == ["window"]:
        return _window(argv[1:])
    return _native(_core.cli_main, argv)


def mcp_main() -> int:
    return _native(_core.mcp_main, sys.executable)

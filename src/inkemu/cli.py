import argparse
import json
import shlex
import sys
from pathlib import Path
from typing import Callable

from .clock import parse_duration
from .emu import NET_MODES, SWIPES, Emu

VERBS = ("tap X Y", "tap-text TEXT", "swipe DIR", "hold X Y [MS]", "key KEY", "type TEXT", "answer yes|no",
         "wait DURATION", "lock", "unlock", "reload", "restart", "shot [PATH]")


class UsageError(Exception):
    pass


def _int(s: str) -> int:
    try:
        return int(s)
    except ValueError:
        raise UsageError(f"expected an integer, got {s!r}") from None


def _split_steps(steps: str) -> list[list[str]]:
    lex = shlex.shlex(steps, posix=True, punctuation_chars=";")
    lex.whitespace_split = True
    try:
        tokens = list(lex)
    except ValueError as e:
        raise UsageError(f"cannot parse --steps: {e}") from None
    out: list[list[str]] = [[]]
    for t in tokens:
        if t == ";":
            out.append([])
        else:
            out[-1].append(t)
    return [s for s in out if s]


def _compile(step: list[str], shots: Callable[[str | None], None]) -> Callable[[Emu], None]:
    verb, args = step[0], step[1:]

    def arity(lo: int, hi: int | None = None) -> None:
        hi = lo if hi is None else hi
        if not lo <= len(args) <= hi:
            raise UsageError(f"{verb!r} takes {lo if lo == hi else f'{lo}-{hi}'} argument(s), got {len(args)}")

    if verb == "tap":
        arity(2)
        x, y = _int(args[0]), _int(args[1])
        return lambda e: e.tap(x, y)
    if verb == "tap-text":
        arity(1)
        return lambda e: e.tap_text(args[0])
    if verb == "swipe":
        arity(1)
        if args[0] not in SWIPES:
            raise UsageError(f"swipe direction must be one of {', '.join(SWIPES)}")
        return lambda e: e.swipe(args[0])
    if verb == "hold":
        arity(2, 3)
        x, y = _int(args[0]), _int(args[1])
        ms = _int(args[2]) if len(args) == 3 else 800
        return lambda e: e.hold(x, y, ms)
    if verb == "key":
        arity(1)
        return lambda e: e.key(args[0])
    if verb == "type":
        arity(1)
        return lambda e: e.type_text(args[0])
    if verb == "answer":
        arity(1)
        if args[0] not in ("yes", "no"):
            raise UsageError("answer takes yes or no")
        return lambda e: e.answer(args[0] == "yes")
    if verb == "wait":
        arity(1)
        try:
            parse_duration(args[0])
        except ValueError as e:
            raise UsageError(str(e)) from None
        return lambda e: e.wait(args[0])
    if verb in ("lock", "unlock", "reload", "restart"):
        arity(0)
        return lambda e: getattr(e, verb)()
    if verb == "shot":
        arity(0, 1)
        return lambda e: shots(args[0] if args else None)(e)
    raise UsageError(f"unknown step {verb!r}; verbs: " + "; ".join(VERBS))


def run_steps(emu: Emu, steps: str, out_dir: Path) -> list[Path]:
    """Run `;`-separated steps, then write texts.json. Raises UsageError before running anything if a step is malformed."""
    out_dir = Path(out_dir)
    written: list[Path] = []
    numbered = 0

    def shots(path: str | None) -> Callable[[Emu], None]:
        def take(e: Emu) -> None:
            nonlocal numbered
            if path is None:
                numbered += 1
            written.append(e.save_shot(path or out_dir / f"{numbered:03d}.png"))
        return take

    plan = [_compile(s, shots) for s in _split_steps(steps)]
    out_dir.mkdir(parents=True, exist_ok=True)
    try:
        for fn in plan:
            fn(emu)
    finally:
        (out_dir / "texts.json").write_text(json.dumps(emu.texts(), ensure_ascii=False, indent=2), encoding="utf-8")
    return written


def _parser() -> argparse.ArgumentParser:
    p = argparse.ArgumentParser(prog="inkemu", description="Emulator of Xteink X4 Pro InkApp Lua apps")
    sub = p.add_subparsers(dest="command", required=True)

    def common(sp: argparse.ArgumentParser) -> None:
        sp.add_argument("app", help="app directory with manifest.json")
        sp.add_argument("--net", choices=NET_MODES, default="replay")
        sp.add_argument("--fresh", action="store_true", help="start from an empty state")
        sp.add_argument("--time", help="virtual start time, ISO 8601")
        sp.add_argument("--state", help="state directory (default: .emu/<app_id>)")
        sp.add_argument("--fixtures", help="fixtures directory (default: tests/fixtures/<app_id>)")

    w = sub.add_parser("window", help="open the emulator window")
    common(w)
    w.add_argument("--port", type=int, default=7480, help="daemon port (used from stage 2)")
    r = sub.add_parser("run", help="one-shot run of a step script, no window")
    common(r)
    r.add_argument("--steps", required=True, help="e.g. \"tap-text 'Tap me'; wait 61s; shot\"")
    r.add_argument("--out", default=".emu/run", help="directory for shots and texts.json")
    return p


def main(argv: list[str] | None = None) -> int:
    try:
        args = _parser().parse_args(argv)
    except SystemExit as e:
        return e.code if isinstance(e.code, int) else 2
    try:
        emu = Emu(args.app, state_dir=args.state, fixtures_dir=args.fixtures, net=args.net, fresh=args.fresh,
                  start_time=args.time)
    except ValueError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    if args.command == "window":
        from .window import run_window
        run_window(emu)
        return 1 if emu.errors else 0
    try:
        written = run_steps(emu, args.steps, Path(args.out))
    except UsageError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    except (LookupError, RuntimeError, ValueError) as e:
        print(f"inkemu: step failed: {e}", file=sys.stderr)
        return 1
    for p in written:
        print(p)
    for w in emu.warnings:
        print(f"WARN: {w}", file=sys.stderr)
    for err in emu.errors:
        print(f"ERROR: {err}", file=sys.stderr)
    return 1 if emu.errors else 0

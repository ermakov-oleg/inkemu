import argparse
import json
import shlex
import sys
from pathlib import Path

from .daemon import (DEFAULT_PORT, SPEC, DaemonUnreachable, UsageError, WindowUnavailable, execute, parse_words,
                     post_command, read_info, serve)
from .emu import NET_MODES, Emu


def _split_steps(steps: str) -> list[list[str]]:
    lex = shlex.shlex(steps, posix=True, punctuation_chars=";")
    lex.whitespace_split = True
    try:
        tokens = list(lex)
    except ValueError as e:
        raise UsageError(f"cannot parse --steps: {e}") from None
    out: list[list[str]] = [[]]
    for t in tokens:
        if set(t) == {";"}:  # shlex glues ";;" into one token
            out.append([])
        else:
            out[-1].append(t)
    return [s for s in out if s]


PRINTED_IN_RUN = ("shot", "texts", "log", "state")


def format_result(cmd: str, result: dict) -> str:
    """What the client prints for one command result."""
    if cmd == "shot" and result.get("shot"):
        return result["shot"]
    if cmd == "texts":
        return "\n".join(f"{t['x']},{t['y']} {t['text']}" for t in result["texts"])
    if cmd == "log":
        return "\n".join(result.get("log", []))
    return json.dumps({k: result[k] for k in ("ok", "error", "texts", "errors", "warnings", "status", "log_tail")
                       if k in result}, ensure_ascii=False, indent=2)


def run_steps(emu: Emu, steps: str, out_dir: Path) -> list[Path]:
    """Run `;`-separated steps through daemon.execute, printing shot paths and texts/log/state output in order,
    then write texts.json. Every step is parsed before any runs."""
    out_dir = Path(out_dir)
    plan = [parse_words(s) for s in _split_steps(steps)]
    if not plan:
        raise UsageError("no steps given")
    if any(cmd == "stop" for cmd, _ in plan):
        raise UsageError("'stop' only makes sense against a running daemon")
    written: list[Path] = []
    numbered = 0
    out_dir.mkdir(parents=True, exist_ok=True)
    try:
        for cmd, args in plan:
            if cmd == "shot" and "path" not in args:
                numbered += 1
                args = {"path": str(out_dir / f"{numbered:03d}.png")}
            result = execute(emu, cmd, args)
            if not result["ok"]:
                raise RuntimeError(result["error"])
            if result["shot"]:
                written.append(Path(result["shot"]))
            if cmd in PRINTED_IN_RUN:
                print(format_result(cmd, result), flush=True)
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

    w = sub.add_parser("window", help="open the emulator window and the command daemon")
    common(w)
    w.add_argument("--port", type=int, default=DEFAULT_PORT, help="daemon port")
    sv = sub.add_parser("serve", help="headless daemon in the foreground")
    common(sv)
    sv.add_argument("--port", type=int, default=DEFAULT_PORT, help="daemon port")
    r = sub.add_parser("run", help="one-shot run of a step script, no window")
    common(r)
    r.add_argument("--steps", required=True, help="e.g. \"tap-text 'Tap me'; wait 61s; shot\"")
    r.add_argument("--out", default=".emu/run", help="directory for shots and texts.json")
    for verb, (usage, _, _) in SPEC.items():
        vp = sub.add_parser(verb, help=f"send to the running daemon: {usage}")
        vp.add_argument("words", nargs="*", help="arguments, see usage")
        vp.add_argument("--port", type=int, help="daemon port (default: from daemon.json)")
    return p


def _client(verb: str, words: list[str], port: int | None) -> int:
    try:
        cmd, cmd_args = parse_words([verb, *words])
    except UsageError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    if cmd == "shot" and "path" in cmd_args:
        cmd_args["path"] = str(Path(cmd_args["path"]).absolute())  # the daemon has another cwd
    if port is None:
        info = read_info()
        if not info:
            print("inkemu: no daemon running here; start one with `inkemu serve APP` or `inkemu window APP`",
                  file=sys.stderr)
            return 2
        port = int(info["port"])
    try:
        result = post_command(port, cmd, cmd_args)
    except DaemonUnreachable as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    print(format_result(cmd, result))
    if not result.get("ok"):
        print(f"inkemu: {result.get('error')}", file=sys.stderr)
        return 1
    return 0


def main(argv: list[str] | None = None) -> int:
    try:
        args = _parser().parse_args(argv)
    except SystemExit as e:
        return e.code if isinstance(e.code, int) else 2
    if args.command in SPEC:
        return _client(args.command, args.words, args.port)
    if args.command in ("window", "serve") and (live := read_info()):
        print(f"inkemu: a daemon is already running (pid {live['pid']}, port {live['port']}, app {live['app']})",
              file=sys.stderr)
        return 2
    try:
        emu = Emu(args.app, state_dir=args.state, fixtures_dir=args.fixtures, net=args.net, fresh=args.fresh,
                  start_time=args.time)
    except ValueError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    if args.command in ("window", "serve"):
        try:
            serve(emu, args.port, window=args.command == "window")
        except OSError as e:
            print(f"inkemu: {e}", file=sys.stderr)
            return 2
        except WindowUnavailable as e:
            print(f"inkemu: {e}", file=sys.stderr)
            return 1
        return 1 if emu.errors else 0
    try:
        run_steps(emu, args.steps, Path(args.out))
    except UsageError as e:
        print(f"inkemu: {e}", file=sys.stderr)
        return 2
    except (LookupError, RuntimeError, ValueError) as e:
        print(f"inkemu: step failed: {e}", file=sys.stderr)
        return 1
    for w in emu.warnings:
        print(f"WARN: {w}", file=sys.stderr)
    for err in emu.errors:
        print(f"ERROR: {err}", file=sys.stderr)
    return 1 if emu.errors else 0

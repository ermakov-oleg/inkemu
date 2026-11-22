"""Run the InkApp API conformance checks of tests/conformance against `inkemu.Emu`.

Each check is a tiny app plus a JSON file of expected values; see tests/conformance/README.md. The runner only
uses the `Emu` members that both the pure-Python and the Rust core have (`firmware_api=(2, 33)` included), so it
can pin either one. See `--help`.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import math
import re
import sys
import tempfile
from dataclasses import dataclass, field
from pathlib import Path
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent.parent / "tests" / "conformance"
PRELUDE = "conformance.lua"
PENDING = "pending.json"
DEFAULT_START = "2026-11-20T09:00:00"
DEFAULT_MANIFEST = {"app_id": "conformance", "entry": "index.lua", "api": "2.33", "permissions": []}
SECTION = re.compile(r"^(\d\d|[A-D])$")
SPEC_REF = re.compile(r"^§(\d{1,2}|[A-D])(\.\d+)*$")
DEVIATION = re.compile(r"^D\d+$")
CHECK_KEYS = {
    "spec",
    "about",
    "manifest",
    "files",
    "fixtures",
    "start_time",
    "steps",
    "expect",
    "errors",
    "log",
    "pixels",
}
FIXTURE_KEYS = {"method", "url", "status", "body", "body_hex", "headers", "delay_ms", "match", "request_body"}
RESERVED_FILES = {"manifest.json", "index.lua", PRELUDE}
KEYS = ("up", "down", "left", "right", "ok", "back")
STEP_FORMS = {
    "wait": re.compile(r"^wait (\d+(ms|s|m|h)?)$"),
    "tap": re.compile(r"^tap (\d+) (\d+)$"),
    "key": re.compile(r"^key (" + "|".join(KEYS) + r")$"),
    "answer": re.compile(r"^answer (yes|no)$"),
    "observe": re.compile(r"^observe ([A-Za-z0-9_]+)$"),
    "lock": re.compile(r"^lock$"),
    "unlock": re.compile(r"^unlock$"),
    "restart": re.compile(r"^restart$"),
    "reload": re.compile(r"^reload$"),
}
RECORD = re.compile(r"@conformance (\S+) (.*)$")


class CheckError(Exception):
    """A malformed check, selector or pending list: reported as one line, exit code 2."""


@dataclass
class Check:
    id: str
    section: str
    dir: Path
    lua: Path
    data: dict
    root: Path


@dataclass
class Result:
    id: str
    section: str
    spec: str
    passed: bool
    diffs: list[str] = field(default_factory=list)
    records: dict = field(default_factory=dict)
    status: str = ""
    reason: str = ""


# ---- loading ------------------------------------------------------------------------------------


def _fail(path: Path, msg: str) -> CheckError:
    return CheckError(f"{path.parent.name}/{path.name}: {msg}")


def _validate(path: Path, d: object) -> None:
    if not isinstance(d, dict):
        raise _fail(path, "must be a JSON object")
    if unknown := set(d) - CHECK_KEYS:
        raise _fail(path, f"unknown keys {sorted(unknown)}")
    if not isinstance(d.get("spec"), str) or not SPEC_REF.match(d["spec"]):
        raise _fail(path, 'needs "spec" like "§6"')
    if not isinstance(d.get("manifest", {}), dict):
        raise _fail(path, '"manifest" must be an object')
    files = d.get("files", {})
    if not isinstance(files, dict):
        raise _fail(path, '"files" must be an object')
    for dest, src in files.items():
        parts = Path(dest).parts
        if not isinstance(src, str) or dest in RESERVED_FILES or Path(dest).is_absolute() or ".." in parts:
            raise _fail(path, f"bad files entry {dest!r}")
        if not (path.parent / src).is_file():
            raise _fail(path, f"files entry {dest!r}: {src} not found next to the check")
    fixtures = d.get("fixtures", [])
    if not isinstance(fixtures, list):
        raise _fail(path, '"fixtures" must be a list')
    seen_keys = set()
    for f in fixtures:
        if problem := _fixture_problem(f):
            raise _fail(path, f"fixture {f!r}: {problem}")
        key = (fixture_key(f["method"], f["url"], f.get("request_body", "").encode()), f.get("match", "exact"))
        if key in seen_keys:
            # the core serves the first match, so a second fixture for the same request would never be used
            raise _fail(path, f"two fixtures answer {key[0]!r}")
        seen_keys.add(key)
    if not isinstance(d.get("start_time", DEFAULT_START), str):
        raise _fail(path, '"start_time" must be a string')
    steps = d.get("steps", [])
    if not isinstance(steps, list):
        raise _fail(path, '"steps" must be a list')
    for step in steps:
        word = step.split(" ", 1)[0] if isinstance(step, str) else None
        if word not in STEP_FORMS or not STEP_FORMS[word].match(step):
            raise _fail(path, f"bad step {step!r}")
    expect = d.get("expect", {})
    if not isinstance(expect, dict):
        raise _fail(path, '"expect" must be an object')
    for name, e in expect.items():
        if not isinstance(e, dict) or "value" not in e or set(e) - {"value", "spec", "deviation", "spec_value"}:
            raise _fail(path, f'expect {name!r} needs "value" and only spec/deviation/spec_value')
        if "deviation" in e:
            if not isinstance(e["deviation"], str) or not DEVIATION.match(e["deviation"]) or "spec_value" not in e:
                raise _fail(path, f'expect {name!r}: a deviation needs an id like D1 and "spec_value"')
        elif not isinstance(e.get("spec"), str) or not SPEC_REF.match(e["spec"]):
            raise _fail(path, f'expect {name!r} needs "spec" like "§6" or a deviation')
    errors = d.get("errors", [])
    log = d.get("log", [])
    if not isinstance(errors, list) or not all(isinstance(s, str) for s in errors):
        raise _fail(path, '"errors" must be a list of strings')
    if not isinstance(log, list) or not all(isinstance(s, str) for s in log):
        raise _fail(path, '"log" must be a list of strings')
    for p in d.get("pixels", []):
        if not (isinstance(p, list) and len(p) == 3 and all(type(v) is int for v in p) and p[2] in (0, 1)):
            raise _fail(path, f"bad pixel {p!r}: use [x, y, 1 for ink or 0 for paper]")
    if not (expect or errors or log or d.get("pixels")):
        raise _fail(path, "expects nothing")


def load_check(json_path: Path, root: Path) -> Check:
    lua = json_path.with_suffix(".lua")
    if not lua.is_file():
        raise _fail(json_path, f"{lua.name} is missing")
    try:
        data = json.loads(json_path.read_text(encoding="utf-8"))
    except ValueError as e:
        raise _fail(json_path, f"not valid JSON ({e})") from None
    _validate(json_path, data)
    section = json_path.parent.name
    return Check(f"{section}/{json_path.stem}", section, json_path.parent, lua, data, root)


def _section_checks(root: Path, section_dir: Path) -> list[Check]:
    if section_dir.parent != root or not SECTION.match(section_dir.name):
        raise CheckError(f"{section_dir.name}: not a section directory (two digits or A-D, right under the root)")
    return [load_check(p, root) for p in sorted(section_dir.glob("*.json"))]


def all_checks(root: Path) -> list[Check]:
    checks = []
    for d in sorted(p for p in root.iterdir() if p.is_dir() and SECTION.match(p.name)):
        checks += _section_checks(root, d)
    return checks


def find_root(path: Path) -> Path:
    for p in [path, *path.parents]:
        if (p / PRELUDE).is_file():
            return p
    raise CheckError(f"{path}: not inside a conformance directory (no {PRELUDE} above it)")


def select(selectors: list[str], root: Path = ROOT) -> tuple[Path, list[Check]]:
    """Checks named by paths (root, section dir, check file) or section ids such as 06, 6 or §6."""
    if not selectors:
        return root, all_checks(root)
    chosen: dict[str, Check] = {}
    roots = set()
    for sel in selectors:
        path = Path(sel)
        if path.exists():
            path = path.resolve()
            r = find_root(path if path.is_dir() else path.parent)
            if path.is_dir():
                found = all_checks(r) if path == r else _section_checks(r, path)
            elif path.suffix in (".json", ".lua") and path.with_suffix(".json").is_file():
                found = [load_check(path.with_suffix(".json"), r)]
            else:
                raise CheckError(f"{sel}: not a check (a .lua needs a sibling .json)")
        else:
            sid = sel.removeprefix("§")
            sid = sid.zfill(2) if sid.isdigit() else sid.upper()
            if not SECTION.match(sid) or not (root / sid).is_dir():
                raise CheckError(f"{sel}: no such path or section")
            r, found = root, _section_checks(root, root / sid)
        roots.add(r)
        chosen.update((c.id, c) for c in found)
    if len(roots) > 1:
        raise CheckError("selectors point into different conformance directories")
    return roots.pop(), list(chosen.values())


def load_pending(root: Path) -> dict[str, str]:
    path = root / PENDING
    if not path.is_file():
        return {}
    try:
        pending = json.loads(path.read_text(encoding="utf-8"))
    except ValueError as e:
        raise CheckError(f"{PENDING}: not valid JSON ({e})") from None
    if not isinstance(pending, dict) or not all(isinstance(v, str) and v for v in pending.values()):
        raise CheckError(f'{PENDING}: must map "<SS>/<name>" to a one-line reason')
    known = {c.id for c in all_checks(root)}
    if unknown := sorted(set(pending) - known):
        raise CheckError(f"{PENDING}: unknown checks {unknown}")
    return pending


# ---- building the app -------------------------------------------------------------------------


def _sha(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def _number(v: object) -> bool:
    return isinstance(v, (int, float)) and not isinstance(v, bool)


def _fixture_problem(f: object) -> str | None:
    """Why a fixture entry of a check cannot be written, or None."""
    if not isinstance(f, dict):
        return "must be an object"
    if unknown := set(f) - FIXTURE_KEYS:
        return f"unknown keys {sorted(unknown)}"
    if not isinstance(f.get("method"), str) or not f["method"] or not isinstance(f.get("url"), str):
        return "needs string method and url"
    if "status" in f and not (isinstance(f["status"], int) and not isinstance(f["status"], bool)):
        return "status must be an integer"
    if "delay_ms" in f and not (_number(f["delay_ms"]) and 0 <= f["delay_ms"] < math.inf):
        return "delay_ms must be a number >= 0"
    if f.get("match", "exact") not in ("exact", "prefix", "glob"):
        return "match must be exact, prefix or glob"
    for k in ("body", "body_hex", "request_body"):
        if k in f and not isinstance(f[k], str):
            return f"{k} must be a string"
    if "body" in f and "body_hex" in f:
        return "give body or body_hex, not both"
    if "body_hex" in f:
        try:
            bytes.fromhex(f["body_hex"])
        except ValueError:
            return "body_hex is not hex"
    headers = f.get("headers", {})
    if not isinstance(headers, dict) or not all(isinstance(v, str) for v in headers.values()):
        return "headers must map names to strings"
    return None


def fixture_key(method: str, url: str, request_body: bytes = b"") -> str:
    """The 0.1 request key: method, URL and, for a POST with a body, the body's SHA-256."""
    method = method.upper()
    key = f"{method} {url}"
    if method == "POST" and request_body:
        key += " " + _sha(request_body)
    return key


def fixture_stem(method: str, url: str, request_body: bytes = b"") -> str:
    """The 0.1 recorder's file name: METHOD_host_<8 hex of the fixture key>."""
    host = re.sub(r"[^A-Za-z0-9.-]", "_", urlsplit(url).netloc.rpartition("@")[2]) or "nohost"
    return f"{method.upper()}_{host}_{_sha(fixture_key(method, url, request_body).encode())[:8]}"


def write_fixtures(fixtures: list[dict], fixtures_dir: Path) -> list[Path]:
    """Writes `net/<stem>.json` + `.body` files in the 0.1 fixture format; returns the JSON paths."""
    net = fixtures_dir / "net"
    net.mkdir(parents=True, exist_ok=True)
    written = []
    for f in fixtures:
        method = f["method"].upper()
        req_body = f.get("request_body", "").encode("utf-8")
        stem = fixture_stem(method, f["url"], req_body)
        if f.get("match", "exact") != "exact":
            stem += "_" + f["match"]
        body = bytes.fromhex(f["body_hex"]) if "body_hex" in f else f.get("body", "").encode("utf-8")
        (net / f"{stem}.body").write_bytes(body)
        response = {
            "status": f.get("status", 200),
            "headers": {k.lower(): v for k, v in f.get("headers", {}).items()},
            "body_file": f"{stem}.body",
        }
        if "delay_ms" in f:
            response["delay_ms"] = f["delay_ms"]
        doc = {
            "request": {
                "method": method,
                "url": f["url"],
                "body_sha256": _sha(req_body) if method == "POST" and req_body else None,
                "headers": {},
            },
            "match": f.get("match", "exact"),
            "response": response,
        }
        path = net / f"{stem}.json"
        path.write_text(json.dumps(doc, indent=2, ensure_ascii=False) + "\n", encoding="utf-8")
        written.append(path)
    return written


def build_app(check: Check, app_dir: Path) -> None:
    app_dir.mkdir(parents=True, exist_ok=True)
    manifest = {**DEFAULT_MANIFEST, **check.data.get("manifest", {})}
    (app_dir / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
    (app_dir / "index.lua").write_bytes(check.lua.read_bytes())
    (app_dir / PRELUDE).write_bytes((check.root / PRELUDE).read_bytes())
    for dest, src in check.data.get("files", {}).items():
        p = app_dir / dest
        p.parent.mkdir(parents=True, exist_ok=True)
        p.write_bytes((check.dir / src).read_bytes())


# ---- running and comparing --------------------------------------------------------------------


def parse_records(log: list[str]) -> dict:
    """The `@conformance` lines of the log; a name recorded twice keeps its last value."""
    records = {}
    for line in log:
        if m := RECORD.search(line):
            try:
                records[m[1]] = json.loads(m[2])
            except ValueError:
                records[m[1]] = {"unparsable_record": m[2]}
    return records


def same(a: object, b: object) -> bool:
    """JSON equality that keeps true apart from 1 and 436 apart from 436.0."""
    if type(a) is not type(b):
        return False
    if isinstance(a, float):
        return a == b or (math.isnan(a) and math.isnan(b))
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    return a == b


def _j(v: object) -> str:
    return json.dumps(v, ensure_ascii=False, sort_keys=True)


def _flatten(prefix: str, value: object, out: dict) -> None:
    out[prefix] = value
    if isinstance(value, dict):
        for k, v in value.items():
            _flatten(f"{prefix}.{k}", v, out)


def _run_steps(emu, steps: list[str]) -> dict:
    observed: dict = {}
    for step in steps:
        word, _, rest = step.partition(" ")
        if word == "wait":
            emu.wait(rest)
        elif word == "tap":
            x, y = rest.split()
            emu.tap(int(x), int(y))
        elif word == "key":
            emu.key(rest)
        elif word == "answer":
            emu.answer(rest == "yes")
        elif word == "observe":
            _flatten(rest, emu.status(), observed)
        else:
            getattr(emu, word)()
    return observed


def evaluate(check: Check, records: dict, observed: dict, errors: list[str], log: list[str], image) -> list[str]:
    diffs = []
    values = {**records}
    for k, v in observed.items():
        if k in values:
            diffs.append(f"{k}: both a record and an observed status field")
        values[k] = v
    for name, e in check.data.get("expect", {}).items():
        where = f"{e['deviation']}, contract: {_j(e['spec_value'])}" if "deviation" in e else e["spec"]
        if name not in values:
            diffs.append(f"{name}: no record, expected {_j(e['value'])} ({where})")
        elif not same(values[name], e["value"]):
            diffs.append(f"{name}: expected {_j(e['value'])} ({where}), got {_j(values[name])}")
    want_errors = check.data.get("errors", [])
    if not want_errors:
        diffs += [f"unexpected Lua error: {err}" for err in errors]
    for s in want_errors:
        if not any(s in err for err in errors):
            diffs.append(f"no Lua error contains {s!r}; errors: {_j(errors)}")
    text = "\n".join(log)
    diffs += [f"log has no {s!r}" for s in check.data.get("log", []) if s not in text]
    for x, y, want in check.data.get("pixels", []):
        if image is None:
            diffs.append("pixels: no frame")
            break
        if not (0 <= x < image.width and 0 <= y < image.height):
            diffs.append(f"pixel ({x}, {y}) is outside the {image.width}x{image.height} frame")
            continue
        got = 1 if image.getpixel((x, y)) == 0 else 0
        if got != want:
            names = ("paper", "ink")
            diffs.append(f"pixel ({x}, {y}): expected {names[want]}, got {names[got]}")
    return diffs


def run_check(check: Check) -> Result:
    from inkemu import Emu

    result = Result(check.id, check.section, check.data["spec"], passed=False)
    with tempfile.TemporaryDirectory(prefix="inkemu-conformance-") as tmp:
        t = Path(tmp)
        emu, observed, image, step = None, {}, None, "build"
        try:
            build_app(check, t / "app")
            write_fixtures(check.data.get("fixtures", []), t / "fixtures")
            step = "start"
            emu = Emu(
                t / "app",
                state_dir=t / "state",
                fixtures_dir=t / "fixtures",
                net="replay",
                start_time=check.data.get("start_time", DEFAULT_START),
                fresh=True,
                firmware_api=(2, 33),
            )
            for step in check.data.get("steps", []):
                observed.update(_run_steps(emu, [step]))
            step = "shot"
            image = emu.shot(dialog=False)
        except Exception as e:  # a core that crashes fails the check, the run goes on
            result.diffs.append(f"runner: {type(e).__name__} at {step!r}: {e}")
        log, errors = (list(emu.log), list(emu.errors)) if emu is not None else ([], [])
    result.records = parse_records(log)
    result.diffs += evaluate(check, result.records, observed, errors, log, image)
    result.passed = not result.diffs
    return result


# ---- reporting --------------------------------------------------------------------------------


def classify(results: list[Result], pending: dict[str, str], strict: bool = False) -> None:
    for r in results:
        r.reason = pending.get(r.id, "")
        if r.id in pending and r.passed:
            r.status = "XPASS"
        elif r.id in pending and not strict:
            r.status = "PENDING"
        else:
            r.status = "ok" if r.passed else "FAIL"


def summary(results: list[Result]) -> list[str]:
    """One §20-style line per section; pending checks that still fail are counted apart."""
    lines = []
    for sec in sorted({r.section for r in results}):
        rs = [r for r in results if r.section == sec]
        pend = sum(r.status == "PENDING" for r in rs)
        failed = sum(r.status in ("FAIL", "XPASS") for r in rs)
        n = len(rs) - pend
        tail = f" pending={pend}" if pend else ""
        verdict = f"FAILED {failed} of {n}" if failed else f"passed checks={n}"
        lines.append(f"api2 {sec} {verdict}{tail}")
    return lines


def report_lines(results: list[Result]) -> list[str]:
    lines = []
    for r in results:
        if r.status == "PENDING":
            lines.append(f"PENDING {r.id}  ({r.reason})")
        elif r.status == "XPASS":
            lines.append(f"XPASS {r.id}  (passes now: remove it from {PENDING})")
        else:
            lines.append(f"{r.status} {r.id}" + (f"  (pending: {r.reason})" if r.reason else ""))
        if r.status in ("FAIL", "PENDING"):
            lines += ["    " + d.replace("\n", "\n      ") for d in r.diffs]
    return lines + summary(results)


def main(argv: list[str] | None = None) -> int:
    ap = argparse.ArgumentParser(description="Run the InkApp API conformance checks against inkemu.Emu.")
    ap.add_argument("targets", nargs="*", metavar="PATH_OR_SECTION", help="check files, section dirs or ids (06, A)")
    ap.add_argument("--json", action="store_true", help="print the results as JSON")
    ap.add_argument("--strict", action="store_true", help=f"count the checks of {PENDING} as failures")
    args = ap.parse_args(argv)
    try:
        root, checks = select(args.targets)
        pending = load_pending(root)
    except CheckError as e:
        print(f"error: {e}", file=sys.stderr)
        return 2
    results = [run_check(c) for c in checks]
    classify(results, pending, strict=args.strict)
    code = 1 if any(r.status in ("FAIL", "XPASS") for r in results) else 0
    if args.json:
        out = {
            "results": [
                {k: getattr(r, k) for k in ("id", "spec", "status", "diffs", "reason", "records")} for r in results
            ],
            "summary": summary(results),
            "exit": code,
        }
        print(json.dumps(out, indent=2, ensure_ascii=False))
    else:
        print("\n".join(report_lines(results)))
    return code


if __name__ == "__main__":
    sys.exit(main())

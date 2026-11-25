"""ctx.ui against the oracle: what the reader's own UI library returned and drew for the same scenarios.

Each `fixtures/ui-oracle/<scenario>.api<minor>.json` holds our scenario's steps (`input.steps`) and the reader
library's results (`output`), recorded with a fixed-pitch font (line height 20/24/30, ascent 17/21/26, 10/12/15 px per
character). `fixtures/fonts-fixed/` is that font as BDF, so text widths agree. `runner.lua` replays the steps in
inkemu, and every step is one test: returns, the canvas calls with their exact arguments, `ui:focused()`, and
`ui:layout` boxes must equal the oracle.

Each step is compared in full, except:

- `NEEDS_HOST` steps depend on a font method the core lacks: while it is missing, the `font` the library picked with it
  is left out of the named canvas calls, the rest is compared, and the step is skipped.
- `HOST_REFUSES` lists the canvas calls the core refuses with a Lua error (the oracle's canvas accepted anything); any
  other refusal fails the step.
- `HOST_FAILS` lists the bad canvas values the core reports when the replay's frame ends (the replay runs in one
  frame), which the oracle's canvas took; any other error fails the scenario.
- `PARKED` steps differ from the oracle in a known way that a later task decides. They are checked first, in full, and
  are strict xfails: one that matches, or differs in more than a frame's or a `ui:layout` call's results, fails.

The JSON files hold inputs we wrote and outputs recorded from the reader's library; see the README next to them.
"""

import json
import math
import re
import shutil
import struct
import zlib
from functools import cache
from pathlib import Path

import pytest

from inkemu import Emu

HERE = Path(__file__).parent / "fixtures"
ORACLE = HERE / "ui-oracle"
FONTS = HERE / "fonts-fixed"
SCENARIOS = {p.name.removesuffix(".json"): p for p in sorted(ORACLE.glob("s*.json"))}
IMAGE_MARKER = re.compile(r"@img:(\d+)x(\d+)")

# (scenario, step) -> the font method the step needs, and the stream entries (from 0) whose font it picks
NEEDS_HOST: dict[tuple[str, int], tuple[str, set[int]]] = {
    ("s03_label_button_region.api26", 3): ("layout", {7, 8}),  # the two labels with font_min
    ("s16_edge_inputs.api12", 17): ("layout", {0, 1, 2, 3}),  # labels with font_min where no tier fits
    ("s16_edge_inputs.api26", 17): ("layout", {0, 1, 2, 3}),
}

# (scenario, step) -> the core's error for each canvas call it refuses, in order
LABEL_TEXT_REFUSED = ["string expected, got table", "string expected, got boolean"]
HOST_REFUSES: dict[tuple[str, int], list[str]] = {
    # label text {} and true reach g:text_box unchanged, which takes only strings and numbers
    ("s16_edge_inputs.api12", 16): LABEL_TEXT_REFUSED,
    ("s16_edge_inputs.api26", 16): LABEL_TEXT_REFUSED,
    # a button, switch or label, at top level or as a child, hands a table or boolean text to g:text_box the same way
    **{
        (f"s17_layout_edges.api{minor}", step): refused
        for minor in (15, 26)
        for step, refused in (
            (12, LABEL_TEXT_REFUSED[:1]),
            (16, LABEL_TEXT_REFUSED[:1]),
            (18, LABEL_TEXT_REFUSED[:1] * 2),
            (37, LABEL_TEXT_REFUSED[1:]),
            (41, LABEL_TEXT_REFUSED[:1]),
        )
    },
}

# scenario -> the core's error for each bad canvas value it finds when the replay's frame ends, in order
HOST_FAILS: dict[str, list[str]] = {
    # a field's radius is not checked (s26#62) and reaches g:rect as it is; the core fails the frame on "x"
    **{f"s26_field.api{minor}": ["radius must be a finite number, got 'x'"] for minor in (12, 26)},
}

# (scenario, step) -> how inkemu still differs from the oracle there, until a later task decides
PARKED: dict[tuple[str, int], str] = {}
# the fields a parked step may differ in: what a frame and a ui:layout call show
PARKED_FIELDS = {
    "frame": {"calls", "focused_after", "op_counts", "stream", "stream_count"},
    "layout": {"result"},
}

# step fields that are notes or inputs, not results
IGNORED = {"i", "op", "note", "font_calls", "expect", "expect_focused", "host_errors", "args"}


def lua(v) -> str:
    """`v` (parsed JSON) as a Lua expression."""
    if v is None:
        return "nil"
    if isinstance(v, bool):
        return "true" if v else "false"
    if isinstance(v, int):
        return str(v)
    if isinstance(v, float):
        if math.isnan(v):
            return "(0/0)"
        if math.isinf(v):
            return "math.huge" if v > 0 else "-math.huge"
        return repr(v)
    if isinstance(v, str):
        out = []
        for ch in v:
            if ch in '"\\':
                out.append("\\" + ch)
            elif ord(ch) < 32 or ord(ch) == 127:
                out.append(f"\\{ord(ch):03d}")
            else:
                out.append(ch)
        return '"' + "".join(out) + '"'
    if isinstance(v, list):
        return "{" + ", ".join(lua(x) for x in v) + "}"
    return "{" + ", ".join(f"[{lua(k)}] = {lua(x)}" for k, x in v.items()) + "}"


def images(app: Path, w: int, h: int) -> None:
    """A gray PNG of `w` x `h` in the package, for the steps' `"@img:WxH"` markers (any picture: the oracle compares
    the canvas calls, not pixels)."""
    data = app / "data"
    data.mkdir(exist_ok=True)
    rows = b"".join(b"\0" + bytes((x * 7 + y * 13) % 256 for x in range(w)) for y in range(h))

    def chunk(kind: bytes, body: bytes) -> bytes:
        return struct.pack(">I", len(body)) + kind + body + struct.pack(">I", zlib.crc32(kind + body))

    png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 0, 0, 0, 0))
    png += chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
    (data / f"img_{w}x{h}.png").write_bytes(png)


@cache
def oracle(scenario: str) -> dict:
    return json.loads(SCENARIOS[scenario].read_text(encoding="utf-8"))


@cache
def replay(scenario: str, tmp: Path) -> tuple[list[dict], dict]:
    """Our results for every step of `scenario`, and what the host offers."""
    data = oracle(scenario)
    inp = data["input"]
    app = tmp / scenario / "app"
    app.mkdir(parents=True)
    manifest = {"app_id": "oracle", "entry": "index.lua", "api": inp["api"], "permissions": []}
    (app / "manifest.json").write_text(json.dumps(manifest), encoding="utf-8")
    shutil.copy(ORACLE / "runner.lua", app / "index.lua")
    caps = inp.get("canvas_caps", {})
    steps = {"steps": inp["steps"], "no_pattern": bool(caps.get("no_pattern")), "touch": inp.get("touch", True)}
    (app / "steps.lua").write_text("return " + lua(steps) + "\n", encoding="utf-8")
    for w, h in sorted(set(IMAGE_MARKER.findall(json.dumps(inp["steps"])))):
        images(app, int(w), int(h))
    emu = Emu(app, state_dir=tmp / scenario / "state", fixtures_dir=tmp / scenario / "fx", fresh=True, fonts_dir=FONTS)
    expected = HOST_FAILS.get(scenario, [])
    assert len(emu.errors) == len(expected), emu.errors
    assert all(e in r for e, r in zip(expected, emu.errors, strict=True)), emu.errors
    result = emu.lua_eval("return RESULT")
    ours = [json.loads(line) for line in result.split("\n")]
    assert len(ours) == len(data["output"])
    return ours, {"layout": emu.lua_eval("return HOST_LAYOUT")}


def same(a, b) -> bool:
    """JSON equality that keeps `1`, `1.0` and `true` apart; an empty list equals an empty object."""
    if isinstance(a, (list, dict)) and isinstance(b, (list, dict)) and not a and not b:
        return True
    if type(a) is not type(b):
        return False
    if isinstance(a, dict):
        return a.keys() == b.keys() and all(same(a[k], b[k]) for k in a)
    if isinstance(a, list):
        return len(a) == len(b) and all(same(x, y) for x, y in zip(a, b, strict=True))
    return a == b


def differences(want: dict, got: dict) -> list[tuple[str, str]]:
    """(field, message) for each difference."""
    out = []
    for key, w in want.items():
        if key in IGNORED:
            continue
        g = got.get(key)
        if key == "calls":
            if len(w) != len(g or []):
                out.append((key, f"calls: {len(g or [])} calls, expected {len(w)}"))
                continue
            for n, (wc, gc) in enumerate(zip(w, g, strict=True), 1):
                for k in ("m", "id", "ret", "error"):
                    if k in wc and not same(wc[k], gc.get(k)):
                        name = wc.get("id", wc["m"])
                        out.append(
                            (key, f"call {n} ({name}) {k}: {json.dumps(gc.get(k))}, expected {json.dumps(wc[k])}")
                        )
            continue
        if key == "stream":
            g = g or []
            for n in range(max(len(w), len(g))):
                wc, gc = (w[n] if n < len(w) else None), (g[n] if n < len(g) else None)
                if not same(wc, gc):
                    out.append((key, f"stream[{n}]: {json.dumps(gc)}, expected {json.dumps(wc)}"))
                    break
            if len(w) != len(g):
                out.append((key, f"stream: {len(g)} calls, expected {len(w)}"))
            continue
        if not same(w, g):
            out.append((key, f"{key}: {json.dumps(g)}, expected {json.dumps(w)}"))
    return out


def without_font(stream: list[dict], entries: set[int]) -> list[dict]:
    out = []
    for n, c in enumerate(stream):
        if n in entries and c.get("a") and isinstance(c["a"][-1], dict):
            c = {**c, "a": [*c["a"][:-1], {k: v for k, v in c["a"][-1].items() if k != "font"}]}
        out.append(c)
    return out


def cases():
    for scenario in SCENARIOS:
        for step in oracle(scenario)["output"]:
            yield pytest.param(scenario, step["i"], id=f"{scenario}#{step['i']}")


@pytest.fixture(scope="module")
def tmp(tmp_path_factory) -> Path:
    return tmp_path_factory.mktemp("oracle")


@pytest.mark.parametrize(("scenario", "i"), list(cases()))
def test_step(scenario: str, i: int, tmp: Path):
    ours, host = replay(scenario, tmp)
    want = oracle(scenario)["output"][i - 1]
    got = ours[i - 1]
    title = want.get("note", want["op"])
    refused = got.get("host_errors", [])
    expected = HOST_REFUSES.get((scenario, i), [])
    assert len(refused) == len(expected) and all(e in r for e, r in zip(expected, refused, strict=False)), (
        f"{title}: the core refused {refused}, expected {expected}"
    )
    parked = PARKED.get((scenario, i))
    if parked:
        diff = differences(want, got)
        fields = {f for f, _ in diff}
        assert diff, f"{title}: matches the oracle now; remove its PARKED entry"
        assert fields <= PARKED_FIELDS[want["op"]], f"{title}, beyond what is parked:\n" + "\n".join(m for _, m in diff)
        pytest.xfail(f"parked: {parked}")
    need, entries = NEEDS_HOST.get((scenario, i), (None, set()))
    if need and not host.get(need):
        want = {**want, "stream": without_font(want["stream"], entries)}
        got = {**got, "stream": without_font(got["stream"], entries)}
        diff = differences(want, got)
        assert not diff, f"{title}, without the fonts font:{need} picks:\n" + "\n".join(m for _, m in diff)
        pytest.skip(f"blocked on host: font:{need} is missing (the rest matches)")
    diff = differences(want, got)
    assert not diff, f"{title}:\n" + "\n".join(m for _, m in diff)

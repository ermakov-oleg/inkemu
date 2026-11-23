"""ctx.ui against the oracle: what the reader's own UI library returned and drew for the same scenarios.

Each `fixtures/ui-oracle/<scenario>.api<minor>.json` holds our scenario's steps (`input.steps`) and the reader
library's results (`output`), recorded with a fixed-pitch font (line height 20/24/30, ascent 17/21/26, 10/12/15 px per
character). `fixtures/fonts-fixed/` is that font as BDF, so text widths agree. `runner.lua` replays the steps in
inkemu, and every step is one test: returns, the canvas calls with their exact arguments, `ui:focused()`, and
`ui:layout` boxes must equal the oracle.

A step that reaches a ctx.ui method inkemu does not implement yet is skipped with that method's name. A step that
needs a host function the core lacks is compared without the canvas calls that depend on it, then skipped while the
core lacks it. `PENDING` lists the steps known to fail for a stated reason (a strict xfail: remove the entry once the
step passes).
"""

import json
import math
import shutil
from functools import cache
from pathlib import Path

import pytest

from inkemu import Emu

HERE = Path(__file__).parent / "fixtures"
ORACLE = HERE / "ui-oracle"
FONTS = HERE / "fonts-fixed"
SCENARIOS = {p.name.removesuffix(".json"): p for p in sorted(ORACLE.glob("s*.json"))}
NOT_IMPLEMENTED = "not implemented in inkemu yet"

# (scenario, step) -> why the step fails today
PENDING: dict[tuple[str, int], str] = {
    ("s01_object_frame.api26", 25): "focus moved by handle_event in step 23, not implemented yet",
    ("s01_object_frame.api26", 26): "focus moved by handle_event in step 23, not implemented yet",
    **{
        ("s03_label_button_region.api26", i): "focus set by ui:focus and drawn after a key event, not implemented yet"
        for i in range(8, 42, 3)
    },
}

# (scenario, step) -> the font method the step needs, and the stream entries (from 0) that depend on it
NEEDS_HOST: dict[tuple[str, int], tuple[str, set[int]]] = {
    ("s03_label_button_region.api26", 3): ("layout", {7, 8}),  # the two labels with font_min
}

# step fields that are notes or inputs, not results
IGNORED = {"i", "op", "note", "font_calls", "expect", "expect_focused"}


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
    steps = {"steps": inp["steps"], "no_pattern": bool(caps.get("no_pattern"))}
    (app / "steps.lua").write_text("return " + lua(steps) + "\n", encoding="utf-8")
    emu = Emu(app, state_dir=tmp / scenario / "state", fixtures_dir=tmp / scenario / "fx", fresh=True, fonts_dir=FONTS)
    assert emu.errors == [], emu.errors
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


def drawn(stream: list[dict]) -> list[dict]:
    # how often the library looks up a canvas method is not behaviour
    return [c for c in stream if c.get("op") != "@g_missing_field"]


def differences(want: dict, got: dict) -> list[str]:
    out = []
    lookups = want.get("op_counts", {}).get("@g_missing_field", 0)
    for key, w in want.items():
        if key in IGNORED:
            continue
        g = got.get(key)
        if key == "calls":
            if len(w) != len(g or []):
                out.append(f"calls: {len(g or [])} calls, expected {len(w)}")
                continue
            for n, (wc, gc) in enumerate(zip(w, g, strict=True), 1):
                for k in ("m", "id", "ret", "error"):
                    if k in wc and not same(wc[k], gc.get(k)):
                        out.append(
                            f"call {n} ({wc.get('id', wc['m'])}) {k}: {json.dumps(gc.get(k))}, expected {json.dumps(wc[k])}"
                        )
            continue
        if key == "stream":
            w, g = drawn(w), drawn(g or [])
            for n in range(max(len(w), len(g))):
                wc, gc = (w[n] if n < len(w) else None), (g[n] if n < len(g) else None)
                if not same(wc, gc):
                    out.append(f"stream[{n}]: {json.dumps(gc)}, expected {json.dumps(wc)}")
                    break
            if len(w) != len(g):
                out.append(f"stream: {len(g)} calls, expected {len(w)}")
            continue
        if key == "op_counts":
            w = {k: v for k, v in w.items() if k != "@g_missing_field"}
        if key == "stream_count":
            w = w - lookups
        if not same(w, g):
            out.append(f"{key}: {json.dumps(g)}, expected {json.dumps(w)}")
    return out


def cases():
    for scenario in SCENARIOS:
        for step in oracle(scenario)["output"]:
            reason = PENDING.get((scenario, step["i"]))
            marks = [pytest.mark.xfail(reason=reason, strict=True)] if reason else []
            yield pytest.param(scenario, step["i"], id=f"{scenario}#{step['i']}", marks=marks)


@pytest.fixture(scope="module")
def tmp(tmp_path_factory) -> Path:
    return tmp_path_factory.mktemp("oracle")


@pytest.mark.parametrize(("scenario", "i"), list(cases()))
def test_step(scenario: str, i: int, tmp: Path):
    ours, host = replay(scenario, tmp)
    want = oracle(scenario)["output"][i - 1]
    got = ours[i - 1]
    if NOT_IMPLEMENTED in json.dumps(got):
        missing = sorted({m for m in json.dumps(got).split('"') if NOT_IMPLEMENTED in m})
        pytest.skip(f"needs a later ctx.ui task: {missing[0]}")
    need, masked = NEEDS_HOST.get((scenario, i), (None, set()))
    if need and not host.get(need):
        want = {**want, "stream": [c for n, c in enumerate(want["stream"]) if n not in masked]}
        got = {**got, "stream": [c for n, c in enumerate(got["stream"]) if n not in masked]}
        diff = differences(want, got)
        assert not diff, f"{want.get('note', want['op'])}, without the calls font:{need} decides:\n" + "\n".join(diff)
        pytest.skip(f"blocked on host: font:{need} is missing (the other calls match)")
    diff = differences(want, got)
    assert not diff, f"{want.get('note', want['op'])}:\n" + "\n".join(diff)

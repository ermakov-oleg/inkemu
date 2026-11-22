"""The conformance checks of tests/conformance, and tests of the runner itself on synthetic checks."""

import importlib.util
import json
import shutil
import sys
from pathlib import Path

import pytest

_spec = importlib.util.spec_from_file_location(
    "conformance", Path(__file__).resolve().parent.parent / "tools" / "conformance.py"
)
conformance = importlib.util.module_from_spec(_spec)
sys.modules["conformance"] = conformance  # dataclasses look their module up
_spec.loader.exec_module(conformance)

ROOT = conformance.ROOT
CHECKS = conformance.all_checks(ROOT)
PENDING = conformance.load_pending(ROOT)


@pytest.mark.parametrize(
    "check",
    [
        pytest.param(
            c, id=c.id, marks=[pytest.mark.xfail(strict=True, reason=PENDING[c.id])] if c.id in PENDING else []
        )
        for c in CHECKS
    ],
)
def test_check(check):
    result = conformance.run_check(check)
    assert result.passed, f"{check.id} ({check.data['spec']}):\n" + "\n".join(result.diffs)


# ---- the runner -------------------------------------------------------------------------------


def make_root(tmp_path, checks, pending=None, helpers=None, name="conf"):
    """A conformance dir with the real prelude: checks = {"06/name": (lua, json dict)}."""
    root = tmp_path / name
    root.mkdir()
    shutil.copy(ROOT / conformance.PRELUDE, root / conformance.PRELUDE)
    for cid, (lua, data) in checks.items():
        (root / cid).parent.mkdir(exist_ok=True)
        (root / f"{cid}.lua").write_text(lua, encoding="utf-8")
        (root / f"{cid}.json").write_text(json.dumps(data), encoding="utf-8")
    for name, text in (helpers or {}).items():
        (root / name).write_text(text, encoding="utf-8")
    if pending is not None:
        (root / conformance.PENDING).write_text(json.dumps(pending), encoding="utf-8")
    return root


VALUE_LUA = 'local T = require("conformance")\nfunction on_load(ctx) T.value("x", 1) end\n'


def value_check(value, spec="§12"):
    return (VALUE_LUA, {"spec": spec, "expect": {"x": {"value": value, "spec": spec}}})


def run_cli(capsys, *args):
    code = conformance.main([str(a) for a in args])
    out = capsys.readouterr()
    return code, out.out, out.err


def test_prelude_encodes_values_calls_and_types(tmp_path):
    lua = r"""
local T = require("conformance")
function on_load(ctx)
  T.value("nested", { a = { 1, 2, { b = false } }, z = "q\"\\\n\1\u{44F}" })
  T.value("empty", {})
  T.value("sparse", { [1] = "a", [3] = "c" })
  T.value("float", 1.5)
  T.value("whole_float", 2.0)
  T.value("int", -7)
  T.value("nil", nil)
  T.value("binary", "\255\0")
  T.call("rets", function() return nil, "x", nil end)
  T.call("none", function() end)
  T.call("raises", function() error("boom") end)
  T.type("t_int", 3)
  T.type("t_float", 3.0)
  T.type("t_print", print)
  T.value("hex", T.hex("\0\171"))
end
"""
    want = {
        "nested": {"a": [1, 2, {"b": False}], "z": 'q"\\\n\x01\u044f'},
        "empty": [],
        "sparse": {"1": "a", "3": "c"},
        "float": 1.5,
        "whole_float": 2.0,
        "int": -7,
        "nil": None,
        "binary": {"invalid_utf8_hex": "ff00"},
        "rets": {"ok": True, "ret": [None, "x", None]},
        "none": {"ok": True, "ret": []},
        "raises": {"ok": False, "err": "boom"},
        "t_int": "integer",
        "t_float": "float",
        "t_print": "nil",
        "hex": "00ab",
    }
    expect = {k: {"value": v, "spec": "§1"} for k, v in want.items()}
    root = make_root(tmp_path, {"01/enc": (lua, {"spec": "§1", "expect": expect})})
    [check] = conformance.all_checks(root)
    result = conformance.run_check(check)
    assert result.passed, result.diffs
    assert isinstance(result.records["whole_float"], float) and isinstance(result.records["int"], int)


def test_same_keeps_bools_and_number_kinds_apart():
    assert conformance.same({"a": [1, 2.5]}, {"a": [1, 2.5]})
    assert not conformance.same(True, 1)
    assert not conformance.same(436, 436.0)
    assert not conformance.same([1], [1, 2])
    assert conformance.same(float("nan"), float("nan"))


def test_failure_prints_diff_and_section_line(tmp_path, capsys):
    root = make_root(tmp_path, {"12/one": value_check(2), "12/two": value_check(1), "03/three": value_check(1)})
    code, out, _ = run_cli(capsys, root)
    assert code == 1
    assert "FAIL 12/one\n    x: expected 2 (§12), got 1" in out
    assert "ok 12/two" in out
    assert "api2 12 FAILED 1 of 2" in out
    assert "api2 03 passed checks=1" in out


def test_errors_pixels_log_and_observe(tmp_path):
    lua = """
function on_load(ctx) ctx.log:info("hello there") end
function on_draw(ctx, g) g:clear(0); g:rect(0, 0, 10, 10, { fill = true }) end
"""
    data = {
        "spec": "§6",
        "steps": ["wait 1s", "observe s"],
        "expect": {
            "s.running": {"value": True, "spec": "§2"},
            "s.screen": {"value": [480, 800], "spec": "§3"},
            "s.time": {"value": "2026-11-20T09:00:01.000", "spec": "§12"},
        },
        "log": ["hello there"],
        "pixels": [[5, 5, 1], [15, 15, 0]],
    }
    root = make_root(tmp_path, {"06/good": (lua, data)})
    assert conformance.run_check(conformance.all_checks(root)[0]).passed

    bad = {**data, "pixels": [[5, 5, 0], [999, 0, 1]], "log": ["absent"], "errors": ["nope"]}
    root2 = make_root(tmp_path, {"06/bad": (lua, bad)}, name="conf2")
    diffs = conformance.run_check(conformance.all_checks(root2)[0]).diffs
    assert "pixel (5, 5): expected paper, got ink" in diffs
    assert "pixel (999, 0) is outside the 480x800 frame" in diffs
    assert "log has no 'absent'" in diffs
    assert any(d.startswith("no Lua error contains 'nope'") for d in diffs)


def gray_diffs(tmp_path, pixels, image):
    data = {"spec": "§3", "pixels": pixels}
    root = make_root(tmp_path, {"03/gray": (VALUE_LUA, data)}, name=f"conf{len(list(tmp_path.iterdir()))}")
    return conformance.evaluate(conformance.all_checks(root)[0], {}, {}, [], [], image)


def gray_shot():
    from PIL import Image

    img = Image.new("L", (4, 1))
    img.putdata([0, 85, 170, 255])
    return img


def test_gray_level_is_checked_on_a_gray_frame(tmp_path):
    ok = [[0, 0, 1, 3], [1, 0, 0, 2], [2, 0, 0, 1], [3, 0, 0, 0]]
    assert gray_diffs(tmp_path, ok, gray_shot()) == []
    diffs = gray_diffs(tmp_path, [[1, 0, 0, 1], [2, 0, 0, 0], [0, 0, 1]], gray_shot())
    assert diffs == [
        "pixel (1, 0): expected gray level 1, got 2 (value 85)",
        "pixel (2, 0): expected gray level 0, got 1 (value 170)",
    ]
    assert gray_diffs(tmp_path, [[1, 0, 1]], gray_shot()) == ["pixel (1, 0): expected ink, got paper"]


def test_gray_level_is_ignored_on_a_one_bit_frame(tmp_path):
    from PIL import Image

    img = Image.new("1", (2, 1))
    img.putdata([0, 255])
    assert gray_diffs(tmp_path, [[0, 0, 1, 3], [1, 0, 0, 2]], img) == []
    assert gray_diffs(tmp_path, [[0, 0, 0, 0]], img) == ["pixel (0, 0): expected paper, got ink"]


@pytest.mark.parametrize("pixel", [[0, 0, 1, 4], [0, 0, 0, -1], [0, 0, 1, 2], [0, 0, 0, 3], [0, 0, 1, 3, 1]])
def test_bad_gray_pixels_are_malformed(tmp_path, pixel):
    root = make_root(tmp_path, {"03/gray": (VALUE_LUA, {"spec": "§3", "pixels": [pixel]})})
    with pytest.raises(conformance.CheckError, match="bad pixel"):
        conformance.all_checks(root)


def test_lua_error_fails_unless_expected(tmp_path):
    lua = 'function on_load(ctx) error("kaput") end\n'
    root = make_root(
        tmp_path,
        {
            "02/raw": (lua, {"spec": "§2", "pixels": [[0, 0, 0]]}),
            "02/allowed": (lua, {"spec": "§2", "errors": ["kaput"]}),
        },
    )
    results = {c.id: conformance.run_check(c) for c in conformance.all_checks(root)}
    assert any("unexpected Lua error" in d and "kaput" in d for d in results["02/raw"].diffs)
    assert results["02/allowed"].passed


def test_pending_counts_apart_and_strict_fails(tmp_path, capsys):
    root = make_root(tmp_path, {"06/red": value_check(2, "§6"), "06/green": value_check(1, "§6")}, {"06/red": "gap"})
    code, out, _ = run_cli(capsys, root)
    assert code == 0
    assert "PENDING 06/red  (gap)" in out
    assert "api2 06 passed checks=1 pending=1" in out
    code, out, _ = run_cli(capsys, root, "--strict")
    assert code == 1
    assert "FAIL 06/red  (pending: gap)" in out
    assert "api2 06 FAILED 1 of 2" in out


def test_pending_check_that_passes_is_xpass(tmp_path, capsys):
    root = make_root(tmp_path, {"06/fixed": value_check(1, "§6")}, {"06/fixed": "was red"})
    code, out, _ = run_cli(capsys, root)
    assert code == 1
    assert "XPASS 06/fixed" in out
    assert "api2 06 FAILED 1 of 1" in out


def test_unknown_pending_id_is_an_error(tmp_path, capsys):
    root = make_root(tmp_path, {"06/a": value_check(1, "§6")}, {"06/missing": "x"})
    code, _, err = run_cli(capsys, root)
    assert code == 2
    assert "unknown checks ['06/missing']" in err


def test_selection_helpers_and_json(tmp_path, capsys):
    root = make_root(
        tmp_path,
        {"06/a": value_check(1, "§6"), "12/b": value_check(1)},
        helpers={"06/a.helper.lua": "return {}\n"},
    )
    assert [c.id for c in conformance.all_checks(root)] == ["06/a", "12/b"]
    assert [c.id for c in conformance.select(["6"], root)[1]] == ["06/a"]
    assert [c.id for c in conformance.select(["§12"], root)[1]] == ["12/b"]
    assert [c.id for c in conformance.select([str(root / "12")])[1]] == ["12/b"]
    assert [c.id for c in conformance.select([str(root / "06" / "a.lua")])[1]] == ["06/a"]
    with pytest.raises(conformance.CheckError):
        conformance.select([str(root / "06" / "a.helper.lua")])
    with pytest.raises(conformance.CheckError):
        conformance.select(["99"], root)
    code, out, _ = run_cli(capsys, root / "06", "--json")
    doc = json.loads(out)
    assert code == 0 and doc["exit"] == 0
    assert doc["summary"] == ["api2 06 passed checks=1"]
    assert doc["results"][0]["id"] == "06/a" and doc["results"][0]["records"] == {"x": 1}


@pytest.mark.parametrize(
    ("data", "msg"),
    [
        ({"expect": {"x": {"value": 1, "spec": "§1"}}}, '"spec"'),
        ({"spec": "§1", "expect": {"x": {"value": 1}}}, "or a deviation"),
        ({"spec": "§1", "expect": {"x": {"value": 1, "deviation": "D1"}}}, "spec_value"),
        ({"spec": "§1", "steps": ["jump 3"], "pixels": [[0, 0, 0]]}, "bad step"),
        ({"spec": "§1", "pixels": [[0, 0, 2]]}, "bad pixel"),
        ({"spec": "§1", "expect": {}, "typo": 1}, "unknown keys"),
        ({"spec": "§1"}, "expects nothing"),
        ({"spec": "§1", "files": {"a.lua": "absent.lua"}, "pixels": [[0, 0, 0]]}, "not found"),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "body_hex": "zz"}],
            },
            "body_hex is not hex",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "headers": ["a"]}],
            },
            "headers must map",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "status": True}],
            },
            "status must be an integer",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "delay_ms": -1}],
            },
            "delay_ms",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "match": "regex"}],
            },
            "match must be",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "body": "a", "body_hex": "00"}],
            },
            "give one of body",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "body_repeat": ["x", -1]}],
            },
            "body_repeat must be",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [{"method": "GET", "url": "https://x.test/a", "body_repeat": ["abcd", 5 * 1024 * 1024]}],
            },
            "at most 16 MiB",
        ),
        (
            {
                "spec": "§1",
                "pixels": [[0, 0, 0]],
                "fixtures": [
                    {"method": "GET", "url": "https://x.test/a"},
                    {"method": "get", "url": "https://x.test/a"},
                ],
            },
            "two fixtures",
        ),
        ({"spec": "§1", "pixels": [[0, 0, 0]], "fixtures": {"method": "GET"}}, "must be a list"),
    ],
)
def test_malformed_checks_are_rejected(tmp_path, data, msg):
    root = make_root(tmp_path, {"01/bad": (VALUE_LUA, data)})
    with pytest.raises(conformance.CheckError, match=msg):
        conformance.all_checks(root)


def test_fixture_files_follow_the_recorder_format(tmp_path):
    paths = conformance.write_fixtures(
        [
            {"method": "get", "url": "https://x.test/a?b=1", "body_hex": "00ff", "headers": {"Content-Type": "a/b"}},
            {"method": "POST", "url": "https://x.test/a?b=1", "request_body": "q=1", "status": 201, "delay_ms": 5},
        ],
        tmp_path,
    )
    get, post = (json.loads(p.read_text(encoding="utf-8")) for p in paths)
    assert paths[0].parent == tmp_path / "net"
    assert paths[0].name.startswith("GET_x.test_") and paths[1].name.startswith("POST_x.test_")
    assert get["request"] == {"method": "GET", "url": "https://x.test/a?b=1", "body_sha256": None, "headers": {}}
    assert get["response"]["headers"] == {"content-type": "a/b"}
    assert (tmp_path / "net" / get["response"]["body_file"]).read_bytes() == b"\x00\xff"
    assert post["request"]["body_sha256"] == conformance._sha(b"q=1")
    assert post["response"]["status"] == 201 and post["response"]["delay_ms"] == 5


def test_fixture_body_repeat(tmp_path):
    (path,) = conformance.write_fixtures(
        [{"method": "GET", "url": "https://x.test/a", "body_repeat": ["ab", 3]}], tmp_path
    )
    doc = json.loads(path.read_text(encoding="utf-8"))
    assert (tmp_path / "net" / doc["response"]["body_file"]).read_bytes() == b"ababab"


def test_fixtures_replay_through_the_core(tmp_path):
    lua = """
local T = require("conformance")
function on_load(ctx)
  ctx.net:request({ url = "https://x.test/a" }, function(c, resp)
    T.value("status", resp.status)
    T.value("body", resp.body)
    T.value("type", resp.headers["content-type"])
  end)
end
"""
    data = {
        "spec": "§14",
        "manifest": {"permissions": ["net"]},
        "fixtures": [
            {
                "method": "GET",
                "url": "https://x.test/a",
                "status": 203,
                "body": "hi",
                "headers": {"Content-Type": "t/p"},
            }
        ],
        "steps": ["wait 1s"],
        "expect": {
            "status": {"value": 203, "spec": "§14"},
            "body": {"value": "hi", "spec": "§14"},
            "type": {"value": "t/p", "spec": "§14"},
        },
    }
    root = make_root(tmp_path, {"14/replay": (lua, data)})
    result = conformance.run_check(conformance.all_checks(root)[0])
    assert result.passed, result.diffs


def test_core_exception_fails_the_check_and_the_run_goes_on(tmp_path, monkeypatch):
    root = make_root(tmp_path, {"06/a": value_check(1, "§6")})
    import inkemu

    def boom(*a, **kw):
        raise RuntimeError("core down")

    monkeypatch.setattr(inkemu, "Emu", boom)
    result = conformance.run_check(conformance.all_checks(root)[0])
    assert not result.passed
    assert result.diffs[0] == "runner: RuntimeError at 'start': core down"


def test_steps_dispatch_to_emu_members():
    calls = []

    class Fake:
        def __getattr__(self, name):
            return lambda *a: calls.append((name, *a))

        def status(self):
            return {"locked": False, "pending": {"kind": "confirm"}}

    steps = [
        "wait 2s",
        "tap 3 4",
        "key ok",
        "answer yes",
        "answer no",
        "lock",
        "unlock",
        "restart",
        "reload",
        "observe o",
    ]
    observed = conformance._run_steps(Fake(), steps)
    assert calls == [
        ("wait", "2s"),
        ("tap", 3, 4),
        ("key", "ok"),
        ("answer", True),
        ("answer", False),
        ("lock",),
        ("unlock",),
        ("restart",),
        ("reload",),
    ]
    assert observed == {
        "o": {"locked": False, "pending": {"kind": "confirm"}},
        "o.locked": False,
        "o.pending": {"kind": "confirm"},
        "o.pending.kind": "confirm",
    }


def test_prelude_records_from_a_lock_screen_session(tmp_path):
    lock = """
local T = require("conformance")
function on_draw(c, g)
  T.value("lock.reason", c.lock.reason)
  c.lock:done()
end
"""
    data = {
        "spec": "§15",
        "manifest": {"lockscreen": {"enabled": True, "entry": "lockscreen.lua", "interval_sec": 60, "net": False}},
        "files": {"lockscreen.lua": "lock.lockscreen.lua"},
        "steps": ["lock", "observe locked", "wait 61s", "unlock", "observe s"],
        "expect": {
            "lock.reason": {"value": "timer", "spec": "§15"},
            "locked.locked": {"value": True, "spec": "§15"},
            "s.locked": {"value": False, "spec": "§15"},
        },
    }
    root = make_root(tmp_path, {"15/lock": (VALUE_LUA, data)}, helpers={"15/lock.lockscreen.lua": lock})
    result = conformance.run_check(conformance.all_checks(root)[0])
    assert result.passed, result.diffs


def test_bad_fixture_exits_2_with_a_message(tmp_path, capsys):
    data = {
        "spec": "§14",
        "pixels": [[0, 0, 0]],
        "fixtures": [{"method": "GET", "url": "https://x.test/", "body_hex": "q"}],
    }
    root = make_root(tmp_path, {"14/bad": (VALUE_LUA, data)})
    code, out, err = run_cli(capsys, root)
    assert code == 2 and out == ""
    assert err.startswith("error: 14/bad.json: fixture ") and "body_hex is not hex" in err


def test_fixtures_differing_only_in_match_or_post_body_are_kept_apart(tmp_path):
    fixtures = [
        {"method": "GET", "url": "https://x.test/a"},
        {"method": "GET", "url": "https://x.test/a", "match": "prefix"},
        {"method": "POST", "url": "https://x.test/a", "request_body": "1"},
        {"method": "POST", "url": "https://x.test/a", "request_body": "2"},
    ]
    root = make_root(tmp_path, {"14/ok": (VALUE_LUA, {"spec": "§14", "pixels": [[0, 0, 0]], "fixtures": fixtures})})
    conformance.all_checks(root)
    assert len({p.name for p in conformance.write_fixtures(fixtures, tmp_path / "fx")}) == 4


def test_prelude_refuses_record_names_with_spaces(tmp_path):
    lua = 'local T = require("conformance")\nfunction on_load(ctx) T.value("a b", 1) end\n'
    data = {"spec": "§1", "errors": ["record name must be a non-empty string without spaces"]}
    root = make_root(tmp_path, {"01/names": (lua, data)})
    result = conformance.run_check(conformance.all_checks(root)[0])
    assert result.passed, result.diffs
    assert result.records == {}


def test_emu_gets_the_documented_constructor_arguments(tmp_path, monkeypatch):
    import inkemu

    seen = {}

    def fake(app, **kw):
        seen.update(kw)
        raise RuntimeError("stop")

    monkeypatch.setattr(inkemu, "Emu", fake)
    root = make_root(tmp_path, {"06/a": value_check(1, "§6")})
    conformance.run_check(conformance.all_checks(root)[0])
    assert set(seen) == {"state_dir", "fixtures_dir", "net", "start_time", "fresh", "firmware_api"}
    assert (seen["net"], seen["fresh"], seen["firmware_api"]) == ("replay", True, (2, 33))

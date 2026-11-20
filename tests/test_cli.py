import json
from pathlib import Path

import pytest

from inkemu.cli import main

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


def run(tmp_path, app, steps, *extra):
    return main(["run", str(app), "--steps", steps, "--out", str(tmp_path / "out"), "--fresh",
                 "--state", str(tmp_path / "state"), "--fixtures", str(tmp_path / "fx"), *extra])


def test_run_steps_hello(tmp_path):
    assert run(tmp_path, HELLO, "tap-text 'Tap me'; shot") == 0
    out = tmp_path / "out"
    texts = json.load(open(out / "texts.json"))
    assert any(t["text"] == "Taps: 1" for t in texts)
    assert (out / "001.png").exists()


def test_shot_with_path_and_numbering(tmp_path):
    target = tmp_path / "x" / "a.png"
    assert run(tmp_path, HELLO, f"shot; shot {target}; shot; wait 1m; swipe up; hold 10 10 700") == 0
    assert target.exists()
    assert (tmp_path / "out" / "001.png").exists() and (tmp_path / "out" / "002.png").exists()


@pytest.mark.allow_errors
def test_run_exit_code_on_lua_error(tmp_path, make_app):
    app = make_app("local x = = 1")
    assert run(tmp_path, app, "shot") == 1


def test_unknown_step_is_usage_error(tmp_path, capsys):
    assert run(tmp_path, HELLO, "jump 1") == 2
    assert "tap-text TEXT" in capsys.readouterr().err
    assert not (tmp_path / "out" / "texts.json").exists()


def test_bad_arguments_are_usage_errors(tmp_path):
    assert run(tmp_path, HELLO, "tap 1") == 2
    assert run(tmp_path, HELLO, "tap a b") == 2
    assert run(tmp_path, HELLO, "wait soon") == 2
    assert run(tmp_path, HELLO, "answer maybe") == 2


def test_missing_text_fails_step_with_code_1(tmp_path, capsys):
    assert run(tmp_path, HELLO, "tap-text Nope; shot") == 1
    assert "Nope" in capsys.readouterr().err
    assert (tmp_path / "out" / "texts.json").exists()

import json
from pathlib import Path

import pytest
from conftest import IMPL

from inkemu.cli import main

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


def run(tmp_path, app, steps, *extra):
    return main(
        [
            "run",
            str(app),
            "--steps",
            steps,
            "--out",
            str(tmp_path / "out"),
            "--fresh",
            "--state",
            str(tmp_path / "state"),
            "--fixtures",
            str(tmp_path / "fx"),
            *extra,
        ]
    )


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_run_steps_hello(tmp_path):
    assert run(tmp_path, HELLO, "tap-text 'Tap me'; shot") == 0
    out = tmp_path / "out"
    texts = json.loads((out / "texts.json").read_text(encoding="utf-8"))
    assert any(t["text"] == "Taps: 1" for t in texts)
    assert (out / "001.png").exists()


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_shot_with_path_and_numbering(tmp_path):
    target = tmp_path / "x" / "a.png"
    assert run(tmp_path, HELLO, f"shot; shot {target}; shot; wait 1m; swipe up; hold 10 10 700") == 0
    assert target.exists()
    assert (tmp_path / "out" / "001.png").exists() and (tmp_path / "out" / "002.png").exists()


@pytest.mark.allow_errors
@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_run_exit_code_on_lua_error(tmp_path, make_app):
    app = make_app("local x = = 1")
    assert run(tmp_path, app, "shot") == 1


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_unknown_step_is_usage_error(tmp_path, capsys):
    assert run(tmp_path, HELLO, "jump 1") == 2
    assert "tap-text TEXT" in capsys.readouterr().err
    assert not (tmp_path / "out" / "texts.json").exists()


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_bad_arguments_are_usage_errors(tmp_path):
    assert run(tmp_path, HELLO, "tap 1") == 2
    assert run(tmp_path, HELLO, "tap a b") == 2
    assert run(tmp_path, HELLO, "wait soon") == 2
    assert run(tmp_path, HELLO, "answer maybe") == 2


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_missing_text_fails_step_with_code_1(tmp_path, capsys):
    assert run(tmp_path, HELLO, "tap-text Nope; shot") == 1
    assert "Nope" in capsys.readouterr().err
    assert (tmp_path / "out" / "texts.json").exists()


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_empty_and_stray_steps(tmp_path, capsys):
    assert run(tmp_path, HELLO, "") == 2
    assert run(tmp_path, HELLO, " ; ") == 2
    assert "no steps" in capsys.readouterr().err
    assert run(tmp_path, HELLO, "; shot ;; wait 1s;") == 0
    assert (tmp_path / "out" / "001.png").exists()


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_shot_write_error_is_exit_1(tmp_path, capsys):
    blocker = tmp_path / "f"
    blocker.write_text("x")
    assert run(tmp_path, HELLO, f"shot {blocker}/a.png") == 1
    assert "step failed" in capsys.readouterr().err


def test_client_without_daemon_is_exit_2(tmp_path, monkeypatch, capsys):
    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))
    assert main(["texts"]) == 2
    assert "no daemon running" in capsys.readouterr().err
    assert main(["tap", "1"]) == 2


def test_client_verbs_against_daemon(tmp_path, monkeypatch, capsys, emu_factory):
    import threading
    import time

    from inkemu import daemon

    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))
    emu = emu_factory(HELLO)
    t = threading.Thread(target=daemon.serve, args=(emu, 0, False), daemon=True)
    t.start()
    for _ in range(500):
        if daemon.read_info(tmp_path / "root"):
            break
        time.sleep(0.02)
    try:
        assert main(["tap-text", "Tap me"]) == 0
        capsys.readouterr()
        assert main(["texts"]) == 0
        assert "Taps: 1" in capsys.readouterr().out
        monkeypatch.chdir(tmp_path)
        assert main(["shot", "rel.png"]) == 0
        assert (tmp_path / "rel.png").exists()
        capsys.readouterr()
        assert main(["tap-text", "nope"]) == 1
        assert main(["wait", "bogus"]) == 2
    finally:
        assert main(["stop"]) == 0
        t.join(10)
    assert daemon.read_info(tmp_path / "root") is None


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_run_prints_texts_log_state_and_shots_in_order(tmp_path, capsys):
    assert run(tmp_path, HELLO, "tap-text 'Tap me'; texts; shot; state; log") == 0
    out = capsys.readouterr().out
    i_texts, i_shot = out.index("Taps: 1"), out.index("001.png")
    i_state, i_log = out.index('"status"'), out.rindex(" started")
    assert i_texts < i_shot < i_state < i_log
    assert any(line.endswith(" Taps: 1") and "," in line for line in out.splitlines())


@pytest.mark.skipif(IMPL == "rust", reason="stage1: task 10")
def test_window_without_display_is_clean_exit_1(tmp_path, monkeypatch, capsys):
    tk = pytest.importorskip("tkinter")
    from inkemu import daemon, window

    monkeypatch.setenv("INKEMU_STATE_ROOT", str(tmp_path / "root"))

    def no_display(*a, **kw):
        raise tk.TclError("no display name and no $DISPLAY environment variable")

    monkeypatch.setattr(window, "build_window", no_display)
    code = main(
        ["window", str(HELLO), "--port", "0", "--state", str(tmp_path / "state"), "--fixtures", str(tmp_path / "fx")]
    )
    err = capsys.readouterr().err
    assert code == 1 and "cannot open the window: no display name" in err and "Traceback" not in err
    assert daemon.read_info(tmp_path / "root") is None

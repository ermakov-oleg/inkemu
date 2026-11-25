import json
import os
import signal
import socket
import subprocess
import sys
from pathlib import Path

import pytest

from inkemu.cli import main

HELLO = Path(__file__).resolve().parent.parent / "examples" / "hello"


@pytest.fixture
def run(tmp_path, cli):
    def go(app, steps, *extra):
        return cli(
            "run",
            app,
            "--steps",
            steps,
            "--out",
            tmp_path / "out",
            "--fresh",
            "--state",
            tmp_path / "state",
            "--fixtures",
            tmp_path / "fx",
            *extra,
        )

    return go


def test_run_steps_hello(tmp_path, run):
    r = run(HELLO, "tap-text 'Tap me'; shot")
    assert r.returncode == 0, r.stderr
    out = tmp_path / "out"
    texts = json.loads((out / "texts.json").read_text(encoding="utf-8"))
    assert any(t["text"] == "Taps: 1" for t in texts)
    assert (out / "001.png").exists()


def test_shot_with_path_and_numbering(tmp_path, run):
    target = tmp_path / "x" / "a.png"
    r = run(HELLO, f"shot; shot {target}; shot; wait 1m; swipe up; hold 10 10 700")
    assert r.returncode == 0, r.stderr
    assert target.exists()
    assert (tmp_path / "out" / "001.png").exists() and (tmp_path / "out" / "002.png").exists()
    assert not (tmp_path / "out" / "003.png").exists()


def test_run_exit_code_on_lua_error(tmp_path, run, make_app):
    app = make_app("local x = = 1")
    r = run(app, "shot")
    assert r.returncode == 1
    assert any(line.startswith("ERROR: ") for line in r.stderr.splitlines())


def test_run_prints_warnings_and_errors_at_the_end(tmp_path, run, make_app):
    app = make_app("function on_load(c) c:log('hi') end\nfunction on_input(c, ev) error('boom') end")
    r = run(app, "tap 5 5; shot")
    assert r.returncode == 1
    errors = [line for line in r.stderr.splitlines() if line.startswith("ERROR: ")]
    assert errors and "boom" in errors[0]


def test_run_prints_warnings(tmp_path, cli, make_app):
    app = make_app("function on_load(c) end")
    state = tmp_path / "state"
    state.mkdir()
    (state / "state.json").write_text("not json", encoding="utf-8")
    r = cli("run", app, "--steps", "shot", "--state", state, "--fixtures", tmp_path / "fx", "--out", tmp_path / "out")
    assert r.returncode == 0, r.stderr
    assert any(line.startswith("WARN: ") and "unreadable state" in line for line in r.stderr.splitlines())


def test_unknown_step_is_usage_error(tmp_path, run):
    r = run(HELLO, "jump 1")
    assert r.returncode == 2
    assert r.stderr.startswith("inkemu: ") and "tap-text TEXT" in r.stderr
    assert not (tmp_path / "out" / "texts.json").exists()


@pytest.mark.parametrize("steps", ["tap 1", "tap a b", "wait soon", "answer maybe"])
def test_bad_arguments_are_usage_errors(run, steps):
    r = run(HELLO, steps)
    assert r.returncode == 2 and r.stderr.startswith("inkemu: ")


def test_missing_text_fails_step_with_code_1(tmp_path, run):
    r = run(HELLO, "tap-text Nope; shot")
    assert r.returncode == 1
    assert r.stderr.startswith("inkemu: step failed: ") and "Nope" in r.stderr
    assert (tmp_path / "out" / "texts.json").exists()
    assert not (tmp_path / "out" / "001.png").exists()


def test_empty_and_stray_steps(tmp_path, run):
    assert run(HELLO, "").returncode == 2
    r = run(HELLO, " ; ")
    assert r.returncode == 2 and "no steps" in r.stderr
    assert run(HELLO, "; shot ;; wait 1s;").returncode == 0
    assert (tmp_path / "out" / "001.png").exists()


def test_shot_write_error_is_exit_1(tmp_path, run):
    blocker = tmp_path / "f"
    blocker.write_text("x")
    r = run(HELLO, f"shot {blocker}/a.png")
    assert r.returncode == 1 and "step failed" in r.stderr


def test_run_usage_errors(tmp_path, run):
    r = run(HELLO, "shot", "--net", "wifi")
    assert r.returncode == 2 and r.stderr
    r = run(tmp_path / "no-such-app", "shot")
    assert r.returncode == 2 and r.stderr == f"inkemu: no such app directory: {tmp_path / 'no-such-app'}\n"
    assert not (tmp_path / "out").exists()
    r = run(HELLO, "shot; stop")
    assert r.returncode == 2
    assert r.stderr.startswith("inkemu: ") and "'stop' only makes sense against a running daemon" in r.stderr
    r = run(HELLO, "tap-text 'unclosed")
    assert r.returncode == 2
    assert r.stderr.startswith("inkemu: ") and "cannot parse --steps" in r.stderr


def test_serve_without_an_app_directory_is_a_usage_error(cli, root, tmp_path):
    r = cli("serve", tmp_path / "no-such-app", "--port", 0)
    assert r.returncode == 2 and r.stderr == f"inkemu: no such app directory: {tmp_path / 'no-such-app'}\n"
    assert not (root / "daemon.json").exists()


@pytest.mark.parametrize(
    "args",
    [
        ["run", str(HELLO), "--ste", "shot"],
        ["run", str(HELLO), "--steps", "shot", "--fre"],
        ["serve", str(HELLO), "--po", "0"],
        ["state", "--por", "7"],
        ["window", str(HELLO), "--fre"],
    ],
)
def test_long_options_are_not_abbreviated(cli, args):
    r = cli(*args)
    assert r.returncode == 2 and r.stderr and r.stdout == ""


def test_there_is_no_help_subcommand(cli):
    r = cli("help")
    assert r.returncode == 2 and "help" in r.stderr
    assert cli("--help").returncode == 0


def test_non_utf8_argument_is_a_usage_error(tmp_path, root):
    r = subprocess.run(
        [sys.executable, "-m", "inkemu", "tap-text", b"x\xff"],
        env={**os.environ, "INKEMU_STATE_ROOT": str(root)},
        cwd=tmp_path,
        capture_output=True,
        timeout=60,
    )
    assert r.returncode == 2, r.stderr
    assert b"Traceback" not in r.stderr and b"no daemon running" not in r.stderr
    assert b"invalid UTF-8" in r.stderr


def test_ctrl_c_stops_a_client_waiting_for_a_silent_daemon(tmp_path, root):
    with socket.socket() as listener:
        listener.bind(("127.0.0.1", 0))
        listener.listen()
        listener.settimeout(20)
        port = listener.getsockname()[1]
        proc = subprocess.Popen(
            [sys.executable, "-m", "inkemu", "texts", "--port", str(port)],
            env={**os.environ, "INKEMU_STATE_ROOT": str(root)},
            cwd=tmp_path,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
        )
        try:
            conn, _ = listener.accept()
            with conn:
                conn.settimeout(20)
                assert conn.recv(4096).startswith(b"POST /cmd ")
                proc.send_signal(signal.SIGINT)
                assert proc.wait(5) != 0
        finally:
            proc.kill()
            proc.wait()


def test_ctrl_c_stops_an_idle_mcp_server(mcp):
    client = mcp()
    assert client.tools()
    client.proc.send_signal(signal.SIGINT)
    client.proc.wait(5)


def test_steps_quoted_semicolon_stays_in_the_word(tmp_path, run):
    r = run(HELLO, "tap-text 'a;b'")
    assert r.returncode == 1
    assert r.stderr.startswith("inkemu: step failed: ") and "a;b" in r.stderr


def test_steps_hash_comments_out_the_rest(tmp_path, run):
    assert run(HELLO, "shot # x; shot").returncode == 0
    assert (tmp_path / "out" / "001.png").exists()
    assert not (tmp_path / "out" / "002.png").exists()


def test_steps_double_semicolon_separates(tmp_path, run):
    assert run(HELLO, "shot;;shot").returncode == 0
    assert (tmp_path / "out" / "002.png").exists()


def test_negative_numbers_are_positional(run):
    r = run(HELLO, "tap -5 10")
    assert r.returncode == 0, r.stderr


def test_run_prints_texts_log_state_and_shots_in_order(tmp_path, run):
    r = run(HELLO, "tap-text 'Tap me'; texts; shot; state; log")
    assert r.returncode == 0, r.stderr
    out = r.stdout
    i_texts, i_shot = out.index("Taps: 1"), out.index("001.png")
    i_state, i_log = out.index('"status"'), out.rindex(" started")
    assert i_texts < i_shot < i_state < i_log
    lines = out.splitlines()
    assert any(line.endswith(" Taps: 1") and "," in line for line in lines)
    assert any(line.endswith("001.png") for line in lines)
    start = lines.index("{")
    end = lines.index("}", start)
    text = "\n".join(lines[start : end + 1])
    assert list(json.loads(text)) == ["ok", "error", "texts", "errors", "warnings", "status", "log_tail"]
    assert text == json.dumps(json.loads(text), indent=2, ensure_ascii=False)
    assert " started" in "\n".join(lines[end + 1 :])


def test_run_relative_out_gives_relative_shot_path(tmp_path, cli):
    r = cli("run", HELLO, "--steps", "shot", "--out", "rel-out", "--fresh", "--state", tmp_path / "state")
    assert r.returncode == 0, r.stderr
    assert r.stdout.strip().endswith("001.png")
    assert (tmp_path / "rel-out" / "001.png").exists()


def test_client_without_daemon_is_exit_2(cli):
    r = cli("texts")
    assert r.returncode == 2 and r.stderr.startswith("inkemu: ") and "no daemon running" in r.stderr
    assert cli("tap", "1").returncode == 2


@pytest.mark.parametrize("content", [json.dumps({"port": 7480, "pid": 999999, "app": "x"}), "garbage"])
def test_stale_daemon_json_is_no_daemon(cli, root, content):
    root.mkdir()
    (root / "daemon.json").write_text(content)
    r = cli("texts")
    assert r.returncode == 2 and "no daemon running" in r.stderr


def test_client_verbs_against_daemon(tmp_path, cli, serve):
    assert cli("tap-text", "Tap me").returncode == 0
    r = cli("texts")
    assert r.returncode == 0 and "Taps: 1" in r.stdout
    r = cli("shot", "rel.png", cwd=tmp_path)
    assert r.returncode == 0
    assert (tmp_path / "rel.png").exists()
    assert r.stdout.strip() == str(tmp_path / "rel.png")
    r = cli("tap-text", "nope")
    assert r.returncode == 1
    assert json.loads(r.stdout)["ok"] is False
    assert r.stderr.startswith("inkemu: ") and "nope" in r.stderr
    assert cli("wait", "bogus").returncode == 2
    assert cli("state", "--port", serve.port).returncode == 0
    assert cli("stop").returncode == 0
    assert serve.wait_exit() == 0
    assert not (serve.root / "daemon.json").exists()


def test_foreign_server_is_not_a_daemon(cli, foreign_server):
    r = cli("state", "--port", foreign_server)
    assert r.returncode == 2 and "did not answer like an inkemu daemon" in r.stderr


def test_nobody_listening(cli):
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        port = s.getsockname()[1]
    r = cli("state", "--port", port)
    assert r.returncode == 2 and "cannot reach the daemon" in r.stderr


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

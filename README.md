# inkemu

Desktop emulator of the Xteink X4 Pro "InkApp" Lua host (XTOS Licorice). It runs an app directory
(`manifest.json` + `index.lua`) on Python + [lupa](https://pypi.org/project/lupa/) (Lua 5.5) + Pillow, renders
480x800 1-bpp frames, and lets you drive the app from a window, a script, a pytest test, a CLI or an MCP client.

The device API is not publicly documented. Everything here is built from community reverse-engineering
notes, so treat it as a development aid, not as a reference of the firmware.

## Install

Clone this repository next to your app project, then from the project:

```bash
uv add --editable ../inkemu
```

Python >= 3.11. The window needs Tk (`python3-tk` on Debian/Ubuntu); everything else is headless.

## Commands

```bash
uv run inkemu window apps/hello             # Tk window + command daemon
uv run inkemu run apps/hello --fresh --steps "tap-text 'Tap me'; wait 61s; shot"   # one-shot, no window
uv run inkemu serve apps/hello              # headless daemon in the foreground
```

Common options: `--net replay|record|live` (default `replay`), `--fresh`, `--time ISO8601` (virtual start time),
`--state DIR`, `--fixtures DIR`. `run` also takes `--steps` and `--out DIR` (default `.emu/run`: numbered
`NNN.png` shots and `texts.json`) and prints shot paths and the output of `texts`, `log` and `state` steps as they
run; it exits 1 if the app raised a Lua error. `window` and `serve` take `--port`
(default 7480, `0` = any free port).

While a daemon runs, the same verbs work as client commands (`uv run inkemu tap-text 'Tap me'`):

`tap X Y`, `tap-text TEXT`, `double-tap X Y`, `swipe up|down|left|right`, `hold X Y [MS]`,
`key up|down|left|right|ok|back`, `type TEXT`, `answer yes|no`, `wait DURATION` (`500ms`, `61s`, `2m`),
`lock`, `unlock`, `reload`, `restart`, `shot [PATH]`, `texts`, `log`, `state`, `resume`, `stop`.

`tap-text` taps the last drawn occurrence of the text; if it is not on screen it fails and lists the visible texts.
Every input advances virtual time by 350 ms. Daemon discovery goes through `<state root>/daemon.json`.

An open dialog (`ctx.dialog:confirm`, `ctx.input:text`/`edit`, the host's "Leave app?") is drawn as a plate with its
title, text or field and `Yes`/`No` or `OK`/`Cancel`, so it shows in shots and `texts`. While it is open, input does
not reach the app (the log says so); tapping a button (`tap-text Yes`) answers it, as do `answer` and `type`.
The window shows the same dialogs as native Tk dialogs instead.

### Window keys

Mouse: click = tap, drag = swipe, long press = hold. Arrows = side buttons, Enter = ok, Esc = back,
`L` lock/unlock, `R` reload the app, `]` +1 minute, `}` +1 hour, `S` save a screenshot, `P` pause/resume the clock.

## Network

`ctx.net:request` goes through one of three modes:

- `replay` (default): answers from fixtures; a request without a fixture gets `err='no_fixture'` and a log line
  naming method, URL and `--net record`. Never touches the network.
- `record`: makes the real request and saves it as a fixture.
- `live`: real requests, nothing saved.

Fixtures live in `<fixtures dir>/net/` (default `tests/fixtures/<app_id>/net/`, relative to the working directory),
one `METHOD_host_hash.json` (request key, response status/headers/`body_file`/`delay_ms`) plus a `.body` file.
POST bodies are matched by SHA-256. A fixture can be written by hand; a malformed one is ignored with a warning.

`<state root>/secrets.json` (default `.emu/secrets.json`) is a flat `{"name": "value"}` object. Values are replaced by
`{{name}}` in recorded URLs, bodies and headers, and `{{name}}` in app files seeded from the project is replaced
back for `live` and `record` requests, including their percent-encoded forms. A password in the URL
(`https://user:pass@host`) is always recorded as `{{userinfo}}`. Sensitive headers (`authorization`, `cookie`,
`*token*`, `*key*`, ...) are never recorded. Keep `secrets.json` out of git.

## State on disk

`<state root>/<app_id>/` holds `state.json` (`ctx.state`), `files/` (`ctx.storage`), `lock.json` and `shots/`.
The state root is `.emu/` in the working directory, or `$INKEMU_STATE_ROOT`. `--fresh` wipes the app state first.

## Tests

```python
from inkemu import Emu

def test_counter(tmp_path):
    emu = Emu("apps/hello", state_dir=tmp_path / "state", fixtures_dir=tmp_path / "fx",
              start_time="2026-11-20T09:00:00")
    assert emu.has_text("Taps: 0")
    emu.tap_text("Tap me")
    assert emu.has_text("Taps: 1")
    emu.wait("61s")
    assert emu.errors == []        # Lua tracebacks are collected, never raised
```

Other members: `emu.texts()`, `emu.log`, `emu.warnings`, `emu.swipe()`, `emu.key()`, `emu.lock()`, `emu.restart()`,
`emu.save_shot(path)`.

## MCP

`inkemu-mcp` is a stdio MCP server. Add to the project's `.mcp.json`:

```json
{"mcpServers": {"inkemu": {"command": "uv", "args": ["run", "inkemu-mcp"]}}}
```

Tools: `emu_start(app, window=None, net=None, fresh=False, port=0)` (starts a detached daemon, headless and `replay`
unless asked otherwise, or reuses the running one for the same app; it refuses with `ok: false` when that daemon
runs in another `window`/`net` mode or `fresh=True` is asked, so call `emu_stop` first), `emu_tap`, `emu_tap_text`, `emu_double_tap`, `emu_swipe`, `emu_hold`, `emu_key`,
`emu_type`, `emu_answer`, `emu_wait`, `emu_lock`, `emu_unlock`, `emu_reload`, `emu_restart`, `emu_screenshot`
(PNG image), `emu_texts`, `emu_log`, `emu_resume`, `emu_stop`. The daemon listens on 127.0.0.1 only and rejects
requests with a foreign `Host`, an `Origin` header or a non-JSON content type.

## Differences from the device

- Fonts are approximate (system Helvetica/Arial/DejaVu with fixed line heights 20/24/30), so text metrics differ.
- Lua integers are 64-bit here; the device is probably 32-bit.
- No `ctx.ui`, no BLE, no WebSocket; only GET and POST get a network callback.
- Layer semantics and error codes are guesses based on the notes.
- Remote commands (daemon verbs, MCP) freeze the window clock, read-only ones included, so a screenshot is stable;
  `resume`, or `P` in the window, lets time run again.
- The frame cap (1024 draw commands) is enforced, extra commands are dropped with a warning.
- A callback (or the lock screen) that runs more than 200 million Lua instructions is aborted as an app error
  ("callback exceeded the instruction budget ... - infinite loop?"); the device has its own, unknown budget.
  Time spent inside C functions such as `string.rep` or pattern matching is not counted.
- The app sees `debug == false` instead of `nil` (lupa reads that global on every call into Lua), and no
  `python`, `io`, `os` or `package.searchpath`.

## Credits

The API notes come from [omer-faruq-xteink/touch-licorice-apps](https://github.com/omer-faruq-xteink/touch-licorice-apps)
(`docs/firmware-notes.md`) and kycube/x4pro-emu. MIT licence, see `LICENSE`.

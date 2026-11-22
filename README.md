# inkemu

[![CI](https://github.com/ermakov-oleg/inkemu/actions/workflows/ci.yml/badge.svg)](https://github.com/ermakov-oleg/inkemu/actions/workflows/ci.yml)
[![PyPI](https://img.shields.io/pypi/v/inkemu)](https://pypi.org/project/inkemu/)
[![Python](https://img.shields.io/pypi/pyversions/inkemu)](https://pypi.org/project/inkemu/)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue)](https://github.com/ermakov-oleg/inkemu/blob/main/LICENSE)

Run and test Lua apps for the Xteink X4 Pro e-ink reader (the "InkApp" host of the XTOS Licorice firmware) on your
desktop. inkemu loads an app folder (`manifest.json` + `index.lua`), renders its 480x800 1-bpp screen and lets you
drive it from a window, a one-shot CLI run, a pytest test or an AI agent over MCP, with virtual time and recorded
network responses.

<p align="center">
  <img src="https://raw.githubusercontent.com/ermakov-oleg/inkemu/main/docs/images/hello.png" width="300" alt="The hello example app: clock, battery and a tap counter">
  &nbsp;&nbsp;
  <img src="https://raw.githubusercontent.com/ermakov-oleg/inkemu/main/docs/images/lockscreen.png" width="300" alt="The lock screen of the hello example">
</p>

The device API is not publicly documented. Everything here is built from community reverse-engineering notes, so
treat inkemu as a development aid, not as a reference of the firmware.

## In the browser

The same core runs as a static page: **<https://ermakov-oleg.github.io/inkemu/>**. It is for trying an InkApp without
Python or the reader. Open an app by pasting a GitHub folder link (`https://github.com/<owner>/<repo>/tree/<ref>/<path>`),
loading a zip, or picking a local folder (re-read it after editing). Click = tap, drag = swipe, long press = hold,
arrows/Enter/Esc = side buttons.

- Share link: `?gh=<owner>/<repo>&ref=<ref>&path=<path>`. It always follows the branch, it is never pinned to a commit.
- Limits: 20 MiB and 2000 files per app; GitHub allows 60 API calls per hour per IP without a token, one per open.
- Fixtures: the nearest `tests/fixtures/<app_id>/` up the tree from the app folder (its `net/` and `reader.json`).
  The default is "fixtures, then the network"; the "fixtures only" toggle replays only.
- Network: the page calls servers from the browser, so a server without CORS headers answers `no_network`. Record a
  fixture with `inkemu run --net record` and keep it in the repository.
- Fonts: the bundled DejaVu Sans is used. "Fonts..." takes your own `small.bdf`, `medium.bdf` and `large.bdf`; the
  reader's font is not included.

Development:

```bash
scripts/build-web.sh                 # builds the Wasm core into web/core/ (needs emscripten)
python3 -m http.server -d web        # then open http://localhost:8000
node --test 'web/tests/**/*.test.mjs'
```

## Install

```bash
uv tool install inkemu      # or: pipx install inkemu
```

or as a dev dependency of your app project: `uv add --dev inkemu` (then prefix the commands with `uv run`).

Python 3.11+, macOS or Linux. Everything is headless except the window, which needs Tk (`python3-tk` on
Debian/Ubuntu). CI runs on uv-managed Pythons (Tk 9); with the python.org builds (Tk 8.6) the window has been seen to
crash on macOS 26.

## Quick start

An app is a folder with `manifest.json` and `index.lua`; [`examples/hello`](https://github.com/ermakov-oleg/inkemu/tree/main/examples/hello)
is a small one with a lock screen (it is not in the package: clone the repository or copy the folder).

```bash
inkemu window examples/hello          # a window: click = tap, drag = swipe, arrows/Enter/Esc = side buttons
```

```bash
# one-shot, no window: numbered PNG shots and texts.json go to .emu/run
inkemu run examples/hello --fresh --time 2026-11-20T09:41 --steps "tap-text 'Tap me'; wait 61s; shot; texts"
```

The same app in a pytest test, with the network answered from recorded fixtures:

```bash
inkemu run apps/weather --net record --steps "wait 5s"    # once: real requests saved to tests/fixtures/<app_id>/net/
```

```python
from inkemu import Emu


def test_weather(tmp_path):
    emu = Emu("apps/weather", state_dir=tmp_path / "state", start_time="2026-11-20T09:00:00")  # net="replay"
    emu.wait("5s")
    assert emu.has_text("Berlin")
    emu.tap_text("Refresh")
    assert emu.errors == []  # Lua tracebacks are collected, never raised
```

Other members: `emu.tap(x, y)`, `emu.double_tap()`, `emu.hold()`, `emu.swipe(direction)`, `emu.key(name)`,
`emu.type_text()`, `emu.answer(ok)`, `emu.lock()`, `emu.unlock()`, `emu.restart()`, `emu.texts()`, `emu.status()`,
`emu.log`, `emu.warnings`, `emu.shot()` (a Pillow image), `emu.save_shot(path)`, `emu.advance(ms)` (moves time
without bumping `clock:now().revision`; the window uses it).

## Commands

```bash
inkemu window APP       # Tk window + command daemon
inkemu run APP --steps "..."   # one-shot, no window
inkemu serve APP        # headless daemon in the foreground
```

Common options: `--net replay|record|live` (default `replay`), `--fresh` (wipe the app state first),
`--time ISO8601` (virtual start time), `--state DIR`, `--fixtures DIR`. `run` also takes `--steps` and `--out DIR`
(default `.emu/run`: numbered `NNN.png` shots and `texts.json`), prints shot paths and the output of `texts`, `log`
and `state` steps as they run, and exits 1 if the app raised a Lua error. `window` and `serve` take `--port`
(default 7480, `0` = any free port).

While a daemon runs, the same verbs work as client commands (`inkemu tap-text 'Tap me'`):

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

## MCP server

`inkemu-mcp` is a stdio MCP server, so an agent can start the app, tap, wait and look at screenshots. In Claude Code,
from your app project:

```bash
claude mcp add inkemu -- inkemu-mcp                     # inkemu installed with uv tool / pipx
claude mcp add --scope project inkemu -- uv run inkemu-mcp   # inkemu is a dev dependency; writes .mcp.json
```

Other clients take the same command, e.g. `{"mcpServers": {"inkemu": {"command": "inkemu-mcp"}}}`. App paths are
relative to the server's working directory, which is the project for Claude Code.

Tools: `emu_start(app, window=None, net=None, fresh=False, port=0)` (starts a detached daemon, headless and `replay`
unless asked otherwise, or reuses the running one for the same app; it refuses with `ok: false` when that daemon
runs in another `window`/`net` mode or `fresh=True` is asked, so call `emu_stop` first), `emu_tap`, `emu_tap_text`,
`emu_double_tap`, `emu_swipe`, `emu_hold`, `emu_key`, `emu_type`, `emu_answer`, `emu_wait`, `emu_lock`,
`emu_unlock`, `emu_reload`, `emu_restart`, `emu_screenshot` (PNG image), `emu_texts`, `emu_log`, `emu_resume`,
`emu_stop`. The daemon listens on 127.0.0.1 only and rejects requests with a foreign `Host`, an `Origin` header or a
non-JSON content type.

## Network

`ctx.net:request` goes through one of three modes:

- `replay` (default): answers from fixtures; a request without a fixture gets `err='no_fixture'` and a log line
  naming method, URL and `--net record`. Never touches the network.
- `record`: makes the real request and saves it as a fixture.
- `live`: real requests, nothing saved.

`live` and `record` do not use the system CA stores or `HTTP(S)_PROXY`.

Fixtures live in `<fixtures dir>/net/` (default `tests/fixtures/<app_id>/net/`, relative to the working directory),
one `METHOD_host_hash.json` (request key, response status/headers/`body_file`/`delay_ms`) plus a `.body` file.
POST bodies are matched by SHA-256. A fixture can be written by hand; a malformed one is ignored with a warning.

`<state root>/secrets.json` (default `.emu/secrets.json`) is a flat `{"name": "value"}` object. Values are replaced by
`{{name}}` in recorded URLs, bodies and headers, and `{{name}}` in app files seeded from the project is replaced
back for `live` and `record` requests, including their percent-encoded forms. A password in the URL
(`https://user:pass@host`) is always recorded as `{{userinfo}}`. Sensitive headers (`authorization`, `cookie`,
`*token*`, `*key*`, ...) are never recorded. Keep `secrets.json` out of git.

## State on disk

`<state root>/<app_id>/` holds `state.json` (`ctx.state`), `files/` (`ctx.storage`), `appfiles/` (`ctx.files`),
`lock.json` and `shots/`. On the device `ctx.storage` and `ctx.files` are two directories too (`XTApps/<app_id>/files/`
and `/AppFiles/<app_id>/`); inkemu applies the quota of 128 MiB and 512 files to each of them. The state root is
`.emu/` in the working directory, or `$INKEMU_STATE_ROOT`. `--fresh` wipes the app state first, both file directories
included, then seeds `files/` from `<fixtures dir>/files/`.

## Supported API

inkemu emulates firmware API 2.33, build 26112001 (`ctx.device.firmware` is `XT Licorice 26112001`, `model`
`x4_pro`, `name` `book`), as inventoried on a reader. A manifest asking for a higher `api` is refused. `Emu(firmware_api=...)` only affects that manifest check and
`ctx.api`; the API surface is always 2.33.

Callbacks: `on_load`, `on_resume`, `on_draw(ctx, g)`, `on_input(ctx, ev)` (pointer taps, double taps, swipes and
holds; side-button keys), `on_tick`, `on_leave`. A `lockscreen` block in the manifest runs its own entry file on
lock and on every `interval_sec` wake, with `ctx.lock` (`reason`, `done`, `set_interval`, ...).

Drawing (`g`): `clear`, `rect`, `round_rect`, `line`, `circle`, `arc`, `polygon`, `polyline`, `text`, `text_box`,
`image`, `pattern`, `pattern_rect`, `fill_pattern`, `size`, `measure`, `measure_text`, plus layers via `ctx.layers`.
The frame cap of 1024 commands is enforced.

Grey, as measured on 26112001: the canvas is 1-bit, and grey is a pattern of black and white pixels.

- `color` is a threshold on the device's scale of 0 (paper) to 15 (ink): 0–7 draw white and 8–15 black in `rect`,
  `circle`, `arc`, `line`, `polygon`, `polyline`, `text` and `text_box`. Without `color` they draw black.
- `g:pattern(x, y, w, h, {kind, density, color})` sets the pattern's pixels to `color` and leaves the others as they
  are, so `color = 0` puts white dots over ink. `kind = 'dot'` is an ordered dither, the standard 8×8 Bayer matrix
  anchored to the screen, with `density` percent of ink. `'line'` inks every other row and `'cross'` is a grid with a
  3 px pitch (about 57 %). Any other kind draws nothing and logs nothing, as on the device.
- `g:rect(x, y, w, h, {radius = r})` rounds the corners; this is how the firmware's own UI draws rounded boxes.
- A grey PNG or JPEG from `ctx.image` is dithered by `g:image` after scaling.

## Fonts

`ctx.fonts.small`, `medium` and `large` on the reader are the firmware's built-in bitmap font: line height 20/24/30,
ascent 17/21/26, proportional digits, no kerning. That font is proprietary and is not shipped with inkemu. By
default inkemu draws text in DejaVu Sans, bundled, at the same line heights; its widths and ascents differ from the
reader's.

If you have the reader's font as BDF files, point inkemu at them: set `INKEMU_FONTS=DIR`, or pass
`Emu(..., fonts_dir=DIR)`. `DIR` must hold `small.bdf`, `medium.bdf` and `large.bdf`; a missing or broken file stops
the start with an error. From each file inkemu takes:

- `FONT_ASCENT` as `ascent` and `FONT_ASCENT` + `FONT_DESCENT` as the line height (else they come from
  `FONTBOUNDINGBOX`; the line height last from `PIXEL_SIZE`);
- each glyph's `DWIDTH` as its advance and its `BBX`/`BITMAP`, drawn pixel for pixel with the baseline at
  `y + ascent`;
- `DEFAULT_CHAR` for characters the file lacks (the reader draws a box for them).

`measure` is the sum of the advances. The same fonts draw the host's dialogs and toasts. The reader's setting InkApp
→ "Fonts" does not change `ctx.fonts`.

`ctx`:

- `invalidate`, `after`/timers, `set_tick`, `defer_full_refresh`, `quit`, `save_state`, `state`, `api`.
- `log`: a table with `error`, `info` and `warn`, each logging with its level (`[app] WARN ...`).
- `capabilities`, `clock` (`now`, `millis`), `device` (`battery`, `firmware`, `model`, `name`, `id_for_app`,
  `lock_timer`), `fonts`.
- `storage` (`read`, `write`, `exists`, `size`, `remove`, `list`, `usage` = `{bytes, files, max_bytes, max_files}`).
- `files` (`read`, `write`, `append`, `exists`, `size`, `remove`, `list`; `path` = `/AppFiles/<app_id>`).
- `package` (`read`, `open_text`, `image`, `exists`, `size`, `cache_stats` = `{}`).
- `net` (`request`; GET and POST), `dialog` (`confirm`, `toast`), `input` (`text`, `edit`, `keyboard_h` = 436,
  `pointer` = true).
- `image`, `qr`, `json`, `base64` (`encode`, `decode`), `hex`, `url` (`encode`, `decode`), `zlib`, `random` (`uuid`, `bytes`).
- `hash`: `sha256`, `md5` and `hmac_sha256` return the raw digest bytes (use `ctx.hex:encode` for text); `sha1` returns the
  20 raw bytes too; `crc32` returns a signed 32-bit integer (values from 2^31 wrap to negative); all verified on the device.
- `screen` (`width`, `height`, `orientation`, `set_orientation`, `gray_levels` = 2, `gray_still` = 4),
  `frontlight` (`get`, `set`, `reset`), `system` (`can_open`, `open`, `act`), `i18n` (`locale`), `perf` (`info` =
  `{}`), `gc`, `budget`, `reader` (answers from `<fixtures dir>/reader.json`), `lockscreen`, `lock`.

`clock:now()` has `year month day hour minute second weekday hour12 date_order date_sep epoch_sec local_sec
offset_minutes revision valid`.

## Differences from the device

- Without BDF files (see Fonts) text is DejaVu Sans, so text widths and ascents differ from the device;
  `ctx.fonts:size(px)` picks the nearest of the three; an app cannot bring its own font.
- Lua is 5.5 with 32-bit integers and 32-bit floats, like the device (`math.maxinteger` is 2147483647,
  `16777217.0 == 16777216.0`).
- The app sees no `io`, `os`, `debug`, `package`, `load`, `loadstring`, `dofile`, `loadfile`, `collectgarbage` or
  `print`, as on the device; `require` searches only the app folder. Lua 5.5's global `warn` is visible to the app.
- Not emulated, so nil: `ctx.crypto`, `ctx.nn`, `ctx.ui`, `ctx.widget`, `dialog:choose`, `i18n:t`, `url:query`,
  `input.keys`, `input.listen`, `input.gamepad`, `device.display`, `device.keep_awake`, `device.radios`,
  `screen.occlusion`, and the fields of `perf:info()` (`asset_hit asset_miss cmd_count draw_frame draw_us image_count
  tick_us` on the device). No BLE, no WebSocket; only GET and POST get a network callback.
- `ctx.capabilities` reports what inkemu emulates: `dialog frontlight image_decode image_transform pattern reader`
  are true, `ble identity nn ui ttf` false. The device reports `ble dialog frontlight identity image_decode
  image_transform nn pattern reader ui` as true and `ttf` as false.
- `ctx.system:act(s)` needs a string ("string expected" otherwise), as on the device. Which strings the device
  accepts is unknown, so inkemu answers every one with `nil, "invalid_argument"` and logs it.
- Not verified on the device:
  - the 2.26 call form `ctx:log(...)`, which still logs;
  - the format of the level prefix of `ctx.log:info/warn/error`;
  - the numbering of `clock:now().weekday` (inkemu: Sunday = 1, as 2.26's `wday`);
  - the meaning of `clock:now().revision` (inkemu: it changes when the clock is moved by a wait, `]` or `}`, not
    when time passes in the window or with an input);
  - the result format of `ctx.hash:hmac_sha256` (raw bytes in inkemu);
  - the level `ctx.frontlight:reset()` goes back to (inkemu: the level at start, 0);
  - `ctx.random:bytes(n)` with `n` below 0 or above 1 MiB: inkemu refuses it with `nil, "invalid_argument"`;
  - the quota of `ctx.files` (inkemu: the same as `ctx.storage`'s, counted separately);
  - `g:pattern` without `density` (inkemu: 50) or without `kind` (inkemu: nothing, like an unknown kind); whether
    `density` changes `line` and `cross` (inkemu: no); whether `line` and `cross` are anchored to the screen like
    `dot` or to the box (inkemu: the screen; the probe's boxes started where both agree);
  - `g:pattern` with a `density` that is not a number (inkemu: the frame fails with "g: bad values in on_draw");
  - the colour threshold in `arc`, `polyline` and `text_box` (the probe tried `rect`, `circle`, `line` and text);
  - `color` outside 0–15: inkemu applies the same threshold (below 8 is white);
  - `g:clear(color)`, which keeps the 0.1 rule (0 white, any other number black);
  - the `fill` option of shapes, which uses Python-style truthiness in inkemu (0, `""` and `{}` are false).
- `g:round_rect`, `g:measure`, `g:measure_text`, `g:fill_pattern` and `g:pattern_rect` are not in the device's `g`
  metatable listing and were not verified by a call; inkemu keeps them (`round_rect` as before; the device's way
  is `g:rect` with `radius`).
- `g:image` resolves the image when the command is recorded, not at the end of `on_draw`.
- Layer semantics and error codes are guesses based on the notes.
- Remote commands (daemon verbs, MCP) freeze the window clock, read-only ones included, so a screenshot is stable;
  `resume`, or `P` in the window, lets time run again.
- A callback (or the lock screen) that runs more than 200 million Lua instructions is aborted as an app error
  ("callback exceeded the instruction budget ... - infinite loop?"); the device has its own, unknown budget.
  Time spent inside C functions such as `string.rep` or pattern matching is not counted.
- Lua `__gc` finalizers run outside the instruction budget, so a finalizer that loops forever hangs the emulator.

## Development

Building needs a Rust toolchain (rustup) and `curl`. `scripts/vendor-lua.sh` downloads Lua 5.5 with the 32-bit number
patch into `vendor/`; run it first on a fresh clone, otherwise Cargo cannot resolve the patched crates.

```bash
scripts/vendor-lua.sh
cargo test
uv sync
uv run pytest
uv run ruff check && uv run ruff format --check
uv run python docs/make_images.py    # regenerate the README screenshots
```

`INKEMU_DEVICE_FONTS=DIR uv run pytest` also checks text widths against a captured device font: `DIR` holds the three
BDF files and `device-fonts.json` with widths measured on a reader. Without it that test is skipped.

## Credits

The API notes come from [omer-faruq-xteink/touch-licorice-apps](https://github.com/omer-faruq-xteink/touch-licorice-apps)
(`docs/firmware-notes.md`) and kycube/x4pro-emu. MIT licence, see
[LICENSE](https://github.com/ermakov-oleg/inkemu/blob/main/LICENSE).

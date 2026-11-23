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
loading a zip, or picking a local folder (re-read it after editing; "Re-read files" needs a Chromium browser, it uses the File System Access API). Click = tap, drag = swipe, long press = hold,
arrows/Enter/Esc = side buttons. The app clock stops while the tab is hidden.

- Share link: `?gh=<owner>/<repo>&ref=<ref>&path=<path>`. It always follows the branch, it is never pinned to a commit.
- Limits: 20 MiB and 2000 files per app; GitHub allows 60 API calls per hour per IP without a token, one call per open for a plain branch name (2–3 when the branch name contains `/`).
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
not reach the app (the log says so); tapping a button (`tap-text Yes`) answers it, as do `answer` and `type`;
`key back` ends an `edit` as Back.
The window shows the same dialogs as native Tk dialogs instead. There, and in the page's text field, the submitted
value replaces an edit's text (it starts prefilled) before the confirm key, while `type` appends to it.

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

## Breaking changes in 0.2

- `g:round_rect`, `g:measure`, `g:measure_text`, `g:fill_pattern` and `g:pattern_rect` are removed because firmware
  26112001 has none of them. Use `g:rect(x, y, w, h, {radius = r})`, `font:measure(text)` (for example
  `ctx.fonts.medium:measure("Hi")`) and `g:pattern`. There is no compatibility mode.
- A `g` kept after its callback returns `nil, "inactive"` instead of drawing into the next frame.
- `g:image` of something that is not an image is `nil, "invalid_argument"`, and of a released `ctx.image` or
  `ctx.qr` image or layer `nil, "invalid_handle"`, instead of drawing nothing or failing the frame (a released
  `ctx.package:image` stays drawable). `layer:draw` on a released layer returns
  `nil, "invalid_handle"` instead of raising, and `release()` returns true.
- `polyline` and `polygon` with too few points or an odd array return `nil, "invalid_argument"` instead of drawing
  nothing or failing the frame; more than 128 points return `nil, "limit_exceeded"`.
- `g:pattern` with an unknown `kind` returns `nil, "invalid_argument"`; without `kind` it draws `dot`.
- `ctx.input:edit`: the edit object's text is the method `ed:text()` (the raw `ed.text` field is gone),
  `ed:close()` returns the final text, `on_end` gets `how = "back"` instead of `"cancel"`, bad options return
  `nil, "invalid_argument"`, and a second edit returns `nil, "busy"` instead of replacing the first. `input:text`
  returns `nil, "busy"` while a text input, an edit or a confirm is open, and `dialog:confirm` while an edit is.
- Circles and arcs follow the pixel rules of §6: half a pixel smaller than in 0.1, an arc of whole turns draws
  nothing, and a filled `arc` is a stroke, not a pie slice.
- `g:text_box` with an `h` below one line height draws nothing and succeeds; 0.1 drew one line. Give it at least
  `font.line_height`. A line that cannot wrap is clipped at the box edge, CJK and Thai text wraps between
  characters, and packages declaring API 2.16 or later get kinsoku.
- `ctx.fonts:size(px)` with `px` outside 8..96 or fractional returns `nil, "invalid_argument"`; 0.1 returned the
  nearest font. Clamp `px` first, or pick `ctx.fonts.small`, `medium` or `large` directly.
- Text metrics changed: the bundled font's ascents are 17/21/26, DejaVu advances are whole pixels and the baseline
  moved, so text shifts and pixel or screenshot baselines of 0.1 no longer match. Regenerate them, and measure
  with `font:measure` instead of assuming widths.
- `g:text`, `g:text_box`, `font:measure` and `font:layout` return `nil, code` for right-to-left text, invalid
  UTF-8 and text over the byte limits (8192 per call, 65536 per frame and per `layout` call). Before, `measure`
  returned a number and the text was drawn or skipped. Check the second result when the text comes from outside.
- `font:measure` and `font:layout` raise `bad argument #1` for anything but a string or a number; 0.1 measured the
  printed form of `nil` or a table. Pass `tostring(v)` or a real string.
- Draw calls after the 1024th command of a frame return `nil, "limit_exceeded"`; 0.1 dropped them
  with only a log warning. Check the result, or draw fewer commands (a layer or one `g:image`).
- `ctx:invalidate(x)` with anything but nothing or `"full"`, and `ctx:defer_full_refresh(ms)` outside 1..60000 or
  fractional, return `nil, "invalid_argument"`, and a refused `invalidate` no longer asks for a frame.

## Supported API

inkemu emulates firmware API 2.33, build 26112001 (`ctx.device.firmware` is `XT Licorice 26112001`, `model`
`x4_pro`, `name` `book`), as inventoried on a reader. A manifest asking for a higher `api` is refused. `Emu(firmware_api=...)` only affects that manifest check and
`ctx.api`; the API surface is always 2.33.

Callbacks: `on_load`, `on_resume`, `on_draw(ctx, g)`, `on_input(ctx, ev)` (pointer taps, double taps, swipes and
holds; side-button keys), `on_tick`, `on_leave`. A `lockscreen` block in the manifest runs its own entry file on
lock and on every `interval_sec` wake, with `ctx.lock` (`reason`, `done`, `set_interval`, ...).

Drawing (`g`): `size`, `clear`, `rect`, `line`, `circle`, `arc`, `polyline`, `polygon`, `pattern`, `text`,
`text_box` and `image`, plus layers via `ctx.layers`. A `g` works only during its callback (`on_draw` or
`layer:draw`); after it returns, every method returns `nil, "inactive"`. A frame accepts 1024 commands, and each
line a `text_box` draws is one of them. A call past the cap returns `nil, "limit_exceeded"` and draws nothing (a
`text_box` that does not fit whole is refused whole); the frame logs one warning with the number of commands asked
for.

Shapes and images, as in the contract (§6):

- `polyline` takes 2 to 128 points and `polygon` 3 to 128, as a flat `{x1, y1, x2, y2, ...}` array (inkemu also
  takes 0.1's `{x, y}` and `{x = , y = }` points). Fewer points or an odd array is `nil, "invalid_argument"`; more
  than 128 points is `nil, "limit_exceeded"`, as on the reader (deviation D2; the contract says `invalid_argument`).
- `circle` fills the pixels whose centre is closer than `r` to `(cx + 0.5, cy + 0.5)`; its stroke is the ring of
  those at `r - line_width` or more. `arc` is that stroke from `start_deg` clockwise to `end_deg` (`fill` does not
  apply); when the two differ by a whole multiple of 360 nothing is drawn.
- `g:pattern`: `kind` defaults to `dot` and `density` to 50. An unknown `kind`, or a `density` that is not a whole
  number from 0 to 100, is `nil, "invalid_argument"` and draws nothing.
- `g:image(img, x, y, opts)` takes an object from `ctx.package:image`, `ctx.image`, `ctx.qr` or `ctx.layers`.
  Anything else, or a layer drawn into itself, is `nil, "invalid_argument"`; a released runtime image or layer is
  `nil, "invalid_handle"`. `src = {x = , y = , w = , h = }`, or `{x, y, w, h}` by position, draws only that region of
  the image: four whole numbers, `x, y >= 0`, `w, h >= 1`, inside the image, else `nil, "invalid_argument"`; `w` and
  `h` then scale the region. `invert = true` swaps black and white after dithering, `radius` clips the output to
  the corners `g:rect{radius}` draws, and a `color` of 0–7 draws the ink of a black-and-white image white.
- `release()` of an image or a layer returns true and can be repeated. A `ctx.package:image` stays drawable after it
  (§8); an image from `ctx.image` or `ctx.qr`, or a layer, is then `nil, "invalid_handle"`. `layer:draw` on a
  released layer returns `nil, "invalid_handle"` without calling the function.

Text, as in the contract (§7):

- `g:text`, `g:text_box` and `font:measure` take up to 8192 bytes per call, `font:layout` up to 65536, and all text
  of one frame up to 65536 bytes; past that the call returns `nil, "limit_exceeded"`. Text that is not valid UTF-8
  is `nil, "invalid_argument"`, and so is text with a right-to-left character; `g:text` and `g:text_box` also log a
  warning.
- A tab is one space; `\n` and other control characters have no width and are not drawn (`text_box` breaks the line
  at `\n`).
- `font:measure(text, {bold, tabular})`: bold adds 1 pixel to a line that has any width; with `tabular` every digit
  takes the width of the widest digit. `italic`, `underline` and `strike` do not change it.
- `g:text_box(x, y, w, h, text, opts)` takes `wrap` (`"word"`, `"char"`, `"none"`), `overflow` (`"clip"`,
  `"ellipsis"`, `"visible"`), `line_gap`, `bold`, `tabular`, `italic`, `underline` and `strike`. `word` breaks at
  spaces and beside CJK and Thai characters and splits a word wider than the box at a code point; whitespace at
  either end of a line takes no width; combining marks and Thai vowels stay with their character; a bold line wraps
  at `w - 1`; from API 2.16 the line-start and line-end punctuation rules move a break back. `clip` keeps the lines
  that fit whole (none is fine: nothing is drawn, the call succeeds) and cuts a line that cannot wrap at the box
  edge; `ellipsis` ends the last line that fits with `…` (`...` in a font without one); `visible` keeps every line.
- `font:layout(text, w, h, opts)` returns `lines, height, consumed`, three integers, by the same rules: `height` is
  `n * line_height + (n - 1) * line_gap` and `consumed` the bytes taken, on a code-point boundary, with the newline
  or the space at the page's end. `lines` is the number of commands the same `text_box` costs.
- `underline` and `strike` draw a line of `max(1, (line_height + 12) // 24)` pixels under or through each line, as
  long as its `measure` width; each is one more command. `italic` slants the glyphs about 18 %. `valign =
  "baseline"` puts `y` on the baseline.
- `bold` and `tabular` work for packages that declare API 2.16 or above, `italic`, `underline` and `strike` for 2.28
  or above; older packages get the plain text.
- `ctx.fonts:size(px, sample)`: `px` is a whole number from 8 to 96 and `sample` nil or text (at most 256 bytes,
  valid UTF-8, no NUL, no right-to-left text), else `nil, "invalid_argument"`; a `sample` that is neither a string
  nor nil (a number included) raises an error. The answer is still the nearest of the three system fonts.

Frames: `emu.status()["refresh"]` is the waveform of the last app frame presented, `"partial"` or `"full"`, and
`emu.status()["refreshes"]` lists the waveforms of the last 64 app frames, oldest first (one step such as a tap can
present more than one frame). The rules, as in the contract:

- `ctx:invalidate()` asks for a partial frame and `ctx:invalidate("full")` or `ctx.system:act("refresh")` for a full
  one; one full request among several makes the frame full. A request made in `on_draw` is for the next frame.
- After 29 partial frames the 30th is full (the firmware's default setting; inkemu does not let you change it). Any
  full frame starts the count again.
- `ctx:defer_full_refresh(ms)` holds that automatic full back for `ms` of virtual time; the first frame after the
  deadline gets the full it is owed, and at 60 partial frames the frame is full even while deferred.
- The first frame of every session (start, restart, reload, unlock, each lock screen session) is full; the contract
  leaves it open, this is inkemu's choice. Lock screen frames count like app frames.
- The error overlay, dialogs and toasts are not frames of the app, and a frame whose `on_draw` fails is not counted.
- `"gray"` is reserved for a frame with a `g:image{gray = true}`; inkemu does not draw gray frames yet.

Grey, as measured on 26112001: the canvas is 1-bit, and grey is a pattern of black and white pixels.

- `color` is a threshold on the device's scale of 0 (paper) to 15 (ink): 0–7 draw white and 8–15 black in `rect`,
  `circle`, `arc`, `line`, `polygon`, `polyline`, `text` and `text_box`. Without `color` they draw black.
- `g:pattern(x, y, w, h, {kind, density, color})` sets the pattern's pixels to `color` and leaves the others as they
  are, so `color = 0` puts white dots over ink. `kind = 'dot'` is an ordered dither, the standard 8×8 Bayer matrix
  anchored to the screen, with `density` percent of ink. `'line'` inks every other row and `'cross'` is a grid with a
  3 px pitch (about 57 %). Any other kind is `nil, "invalid_argument"` and draws nothing, as on the device.
- `g:rect(x, y, w, h, {radius = r})` rounds the corners; this is how the firmware's own UI draws rounded boxes.
- A grey PNG or JPEG from `ctx.image` is dithered by `g:image` after scaling.

## Fonts

`ctx.fonts.small`, `medium` and `large` on the reader are the firmware's built-in bitmap font: line height 20/24/30,
ascent 17/21/26, proportional digits, no kerning. That font is proprietary and is not shipped with inkemu. By
default inkemu draws text in DejaVu Sans, bundled, at the same line heights and ascents (the baseline is drawn at
`y + ascent`); its widths differ from the reader's. Each character advances a whole number of pixels, so `measure`
is the drawn width.

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

- `invalidate` (nothing or `"full"`; another argument is `nil, "invalid_argument"`), `after`/timers, `set_tick`,
  `defer_full_refresh` (whole ms from 1 to 60000, else `nil, "invalid_argument"`), `quit`, `save_state`, `state`,
  `api`.
- `log`: a table with `error`, `info` and `warn`, each logging with its level (`[app] WARN ...`).
- `capabilities`, `clock` (`now`, `millis`), `device` (`battery`, `firmware`, `model`, `name`, `id_for_app`,
  `lock_timer`), `fonts`.
- `storage` (`read`, `write`, `exists`, `size`, `remove`, `list`, `usage` = `{bytes, files, max_bytes, max_files}`).
- `files` (`read`, `write`, `append`, `exists`, `size`, `remove`, `list`; `path` = `/AppFiles/<app_id>`).
- `package` (`read`, `open_text`, `image`, `exists`, `size`, `cache_stats` = `{}`).
- `net` (`request`; GET and POST), `dialog` (`confirm`, `toast`), `input` (`text`, `edit`, `keyboard_h` = 436,
  `pointer` = true). `input:edit` checks its options as §4 (`rect`, `text`, `placeholder`, `max_chars`, `kind`,
  `multiline`, `enter`, else `nil, "invalid_argument"`) and returns `ed` with `set_text`, `text` and `close`;
  `on_end` gets `how` = `"enter"` or `"back"` (the Back key or `answer no`). While it is open, another `edit`,
  `input:text` and `dialog:confirm` return `nil, "busy"`; `edit` and `input:text` are also busy while a text input
  or a confirm is open.
- `image`, `qr`, `json`, `base64` (`encode`, `decode`), `hex`, `url` (`encode`, `decode`), `zlib`, `random` (`uuid`, `bytes`).
- `hash`: `sha256`, `md5` and `hmac_sha256` return the raw digest bytes (use `ctx.hex:encode` for text); `sha1` returns the
  20 raw bytes too; `crc32` returns a signed 32-bit integer (values from 2^31 wrap to negative); all verified in QEMU on firmware 26112001.
- `screen` (`width`, `height`, `orientation`, `set_orientation`, `gray_levels` = 2, `gray_still` = 4),
  `frontlight` (`get`, `set`, `reset`), `system` (`can_open`, `open`, `act`), `i18n` (`locale`), `perf` (`info` =
  `{tick_us, draw_us, cmd_count, image_count, asset_hit, asset_miss, draw_frame}`: `cmd_count` and `image_count`
  are the commands and `g:image` calls accepted in the last frame, `draw_frame` the frames of this session), `gc`,
  `budget`, `reader` (answers from `<fixtures dir>/reader.json`), `lockscreen`, `lock`.
- `ui` (§16), in progress: `create`, `begin_frame`/`end_frame`, `label`, `button`, `region`, `handle_event`,
  `focus`/`focused` and the arrow keys (by position from API 2.13, in declaration order before it) work, and so do
  `row`/`column` (the 2.15 flex layout with `wrap`, `baseline`, `aspect_ratio`, percentages and the container look;
  the earlier rules for packages before 2.15) and `ui:layout`; image and skin buttons, container skins in a frame,
  `grid`/`stack` from 2.15, and the other controls raise "not implemented in inkemu yet".

`clock:now()` has `year month day hour minute second weekday hour12 date_order date_sep epoch_sec local_sec
offset_minutes revision valid`.

## Differences from the device

- Without BDF files (see Fonts) text is DejaVu Sans, so text widths differ from the device (line heights and
  ascents are the device's); `ctx.fonts:size(px)` picks the nearest of the three; an app cannot bring its own font.
- Text rules of §7 not emulated: `align = "justify"` draws as `left`; a character DejaVu Sans lacks draws nothing
  (the device draws a box; BDF files use their `DEFAULT_CHAR`); `small` does not switch to `medium` for CJK; the
  line-break rules know CJK, Thai and Tibetan by code point ranges, not by Unicode's line-breaking classes.
- Lua is 5.5 with 32-bit integers and 32-bit floats, like the device (`math.maxinteger` is 2147483647,
  `16777217.0 == 16777216.0`).
- The app sees no `io`, `os`, `debug`, `package`, `load`, `loadstring`, `dofile`, `loadfile`, `collectgarbage` or
  `print`, as on the device; `require` searches only the app folder. Lua 5.5's global `warn` is visible to the app.
- Not emulated, so nil: `ctx.crypto`, `ctx.nn`, `ctx.ui`, `ctx.widget`, `dialog:choose`, `i18n:t`, `url:query`,
  `input.keys`, `input.listen`, `input.gamepad`, `device.display`, `device.keep_awake`, `device.radios`,
  `screen.occlusion`. No BLE, no WebSocket; only GET and POST get a network callback.
- `ctx.capabilities` reports what inkemu emulates, except that `image_transform` is true as on the device: `dialog frontlight image_decode image_transform pattern reader`
  are true, `ble identity nn ui ttf` false. The device reports `ble dialog frontlight identity image_decode
  image_transform nn pattern reader ui` as true and `ttf` as false.
- Docked input (`ctx.input:edit`) is the dialog plate: the app gets no input while it is open (on the device,
  pointers outside `rect` and the keys other than Back still reach it), the host draws no text in `rect` (it is
  checked and logged), `kind` and `placeholder` are only checked, and there is no composition and no full-screen
  fallback. `type` types the text and presses the confirm key; a newline in it is the confirm key too. With
  `enter = "newline"`, `type` and `answer yes` insert a line break and the field stays open.
- `ctx.system:act(s)` needs a string ("string expected" otherwise), as on the device. `"refresh"` makes the next
  frame full, like `ctx:invalidate("full")`; inkemu answers every other string, `"sleep"` and `"power_off"`
  included, with `nil, "invalid_argument"` and logs it.
- Not verified on the device:
  - the 2.26 call form `ctx:log(...)`, which still logs;
  - `ctx.perf:info()`'s `tick_us`, `draw_us`, `asset_hit` and `asset_miss`, which measure the device's renderer:
    always 0 in inkemu;
  - the format of the level prefix of `ctx.log:info/warn/error`;
  - the numbering of `clock:now().weekday` (inkemu: Sunday = 1, as 2.26's `wday`);
  - the meaning of `clock:now().revision` (inkemu: it changes when the clock is moved by a wait, `]` or `}`, not
    when time passes in the window or with an input);
  - the result format of `ctx.hash:hmac_sha256` (raw bytes in inkemu);
  - the level `ctx.frontlight:reset()` goes back to (inkemu: the level at start, 0);
  - `ctx.random:bytes(n)` with `n` below 0 or above 1 MiB: inkemu refuses it with `nil, "invalid_argument"`;
  - the quota of `ctx.files` (inkemu: the same as `ctx.storage`'s, counted separately);
  - whether `density` changes the `line` and `cross` patterns (inkemu: no); whether `line` and `cross` are anchored
    to the screen like `dot` or to the box (inkemu: the screen; the probe's boxes started where both agree);
  - `g:pattern` with a `density` that is not a number (inkemu: the frame fails with "g: bad values in on_draw");
  - the colour threshold in `arc`, `polyline` and `text_box` (the probe tried `rect`, `circle`, `line` and text);
  - `color` outside 0–15: inkemu applies the same threshold (below 8 is white);
  - `g:clear(color)`, which keeps the 0.1 rule (0 white, any other number black);
  - the `fill` option of shapes, which uses Python-style truthiness in inkemu (0, `""` and `{}` are false);
  - text options the contract leaves open: `line_gap` outside 0–255 (inkemu clamps it), an unknown `wrap` or
    `overflow` (inkemu: the default), a newline at the very end of the text (inkemu: no empty line after it),
    `consumed` when `ellipsis` took characters off the last line (inkemu: the bytes up to the kept ones), a
    `text_box` with no line that fits in a frame that is already full (inkemu: accepted, it costs nothing), a
    `ctx.fonts:size` sample over 256 bytes (inkemu: `invalid_argument`).
- `g:image` resolves the image when the command is recorded, not at the end of `on_draw`.
- Images and layers have no transparency: a new layer is white, not transparent, so a layer covers what lies under
  it and `invert` turns its undrawn area black. `g:image`'s `background` is not emulated (with opaque images there is
  nothing for it to show through), nor are `gray` and the transforms (`flip_x`, `flip_y`, `rotate`, `pivot_x`,
  `pivot_y`): `ctx.capabilities.image_transform` is true as on the device, but these options are ignored and the
  log warns once per session. `w` and `h` scale
  each side on its own, up to 8192 (the contract scales only when both are set, up to 4096), and shrinking the width
  is Lanczos plus dithering, not nearest-neighbour. `color` tints only an image (or `src` region) whose pixels are
  all pure black or white, so an anti-aliased PNG icon is not tinted.
- Layers: `draw` inside `on_draw` or inside another `draw` still runs (the contract returns `nil, "busy"`), an error
  in its function fails the app, and `set_bits` and the limit of 4 layers are not emulated.
- Remote commands (daemon verbs, MCP) freeze the window clock, read-only ones included, so a screenshot is stable;
  `resume`, or `P` in the window, lets time run again.
- A callback (or the lock screen) that runs more than 200 million Lua instructions is aborted as an app error
  ("callback exceeded the instruction budget ... - infinite loop?"); the device has its own, unknown budget.
  Time spent inside C functions such as `string.rep` or pattern matching is not counted.
- Lua `__gc` finalizers run outside the instruction budget, so a finalizer that loops forever hangs the emulator.

## Conformance

`tests/conformance/` holds one small Lua app plus JSON expectations per rule of the official API contract.
`uv run python tools/conformance.py [SECTION ...]` runs them and `uv run pytest -q tests/test_conformance.py` does the
same under pytest. Known core gaps are listed in `tests/conformance/GAPS.md`; places where inkemu follows a measured
reader instead of the contract are in `spec/deviations.md`. `uv run python tools/spec_sync.py` compares the contract
with the lock in `spec/contract.lock` (the lock keeps only hashes, no contract text).

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

# Changelog

## Unreleased

### Breaking changes

The CLI, the command daemon and the MCP server no longer copy the Python quirks of 0.1 and 0.2. README "Breaking
changes in 0.3" has each one with the old and new behaviour.

- `POST /cmd` arguments are typed JSON: `x`, `y` and `ms` are integers (no strings, floats or bools), `text` and
  `path` are strings, `duration` is a string such as `"61s"` or an integer of milliseconds. Errors show the value as
  JSON: `'tap': x must be an integer, got "a"`. CLI words and `--steps` work as before.
- `args` in `POST /cmd` must be an object or `null`; `[]`, `""`, `0` and `false` are a 400 `bad request: args must be
  an object`.
- Shot paths, `--out`, `--state` and `--fixtures` are printed as given, without Python's path normalisation.
- `serve`, `run` and `shot` always write PNG, whatever the extension; a path without an extension is no longer an
  error.
- `daemon.json` is compact JSON, and a `port` or `pid` that is a string makes it unreadable.
- Long options are no longer abbreviated: `--fre` is an error, not `--fresh`. There is no `inkemu help` subcommand
  (`-h` and `--help` stay).
- `run` and `serve` with an app path that is not a directory exit 2 with `inkemu: no such app directory: PATH`
  before starting anything (0.2: exit 1 and a Lua error).
- A failed client command whose reply has no `error` prints `inkemu: the daemon reported an error` (0.2:
  `inkemu: None`).
- MCP `emu_start` refusals name modes as JSON (`window=false`, `fresh=true`, `net=null`), and an unknown `net` reads
  `net must be one of replay, record, live, got "wifi"`.

### Changed

- The CLI (`inkemu run`, `serve` and the client verbs), the command daemon and the MCP server are now Rust, which
  ends the 0.2.0 known limit; `mcp` is no longer a runtime dependency. `inkemu window` is still Python (Tk).
- `inkemu window` still writes the image format its shot path's extension names (Pillow), while `serve` and `run`
  always write PNG.
- Ctrl-C stops `inkemu` client commands and `inkemu-mcp` at once. `inkemu serve` stops with exit 2 if its HTTP server
  stops accepting connections, instead of running on deaf. "cannot reach the daemon" names only the OS error.

## 0.2.0

inkemu now follows the official InkApp API contract 2.33.1, with firmware 26112001 measured on a reader where the two
differ. The emulator core is rewritten in Rust and also runs in the browser.

### Breaking changes

Apps written against 0.1 need changes. README "Breaking changes in 0.2" has each one with the old and new behaviour.
The main ones are:

- `g:round_rect`, `g:measure`, `g:measure_text`, `g:fill_pattern` and `g:pattern_rect` are gone. Use
  `g:rect(x, y, w, h, {radius = r})`, `font:measure(text)` and `g:pattern`.
- Many calls return `nil, code` (`invalid_argument`, `limit_exceeded`, `inactive`, `busy`, `invalid_handle`) where
  0.1 drew, dropped or raised. For example, draw calls past the 1024th command of a frame return `limit_exceeded`,
  `ctx.fonts:size(px)` outside 8..96 returns `invalid_argument`, and `random:bytes(n)` takes 1 to 1024.
- `ctx.log` methods, `font:measure` and `font:layout` raise a Lua error for an argument of the wrong type.
- `ctx.net:request` answers are `{ok, status, headers, body, bytes, saved?}` without `code` and `error`. An answer
  without a response has `ok = false` and no `status` (0.1: `status = 0`). One request runs at a time, and the method
  must be one of the 13 of §14.
- Fixtures key on the body of every method that sends one. Record again the `PUT`, `PATCH`, ... fixtures that have no
  `body_sha256`.
- `Emu.shot()` returns a mode `"L"` image for a gray frame (`g:image{gray = true}`).
- `ctx.storage` and `ctx.files` have their own name rules, per-call limits and `limit_exceeded`. `ctx.files:list()`
  returns `{name, size}` items, and `read` takes `offset, len`.
- `clock:now().weekday` starts at Sunday = 0, and `revision` starts at 1.
- Lock screen: `reason = "first"`, wakes aligned to `interval_sec`, `net_interval_sec`, and a 28 s preparation cap.
- `ctx.input:edit`: `ed:text()` replaces `ed.text`, and `how = "back"` replaces `"cancel"`.
- Text metrics, centring, circles, polygons and thick lines changed, so pixel and screenshot baselines from 0.1 must
  be regenerated.
- The default request `content-type` is `text/plain; charset=utf-8`.

### Added

- Rust core (`crates/inkcore`) shared by the Python package (PyO3) and the page (Wasm). It runs Lua 5.5 with 32-bit
  integers and floats, as measured on the reader. The `lupa` and `segno` dependencies are gone. Wheels are built for
  macOS (arm64, x86_64) and manylinux (x86_64, aarch64).
- The browser page <https://ermakov-oleg.github.io/inkemu/> opens an app from a GitHub folder, a zip or a local
  folder. It replays fixtures, can call the network, and takes your own BDF fonts.
- Optional device fonts: `INKEMU_FONTS=DIR` or `Emu(fonts_dir=...)` with the reader's `small`, `medium` and
  `large` BDF files. The bundled DejaVu Sans is the default; the reader's font is not shipped.
- Gray frames (§3): the `"gray"` refresh kind and 4 ink steps. `Emu.shot()` returns a mode `"L"` image for them and
  `Emu.raw_frame()` a 1-bit one.
- Refresh kinds in `Emu.status()["refresh"]` and `["refreshes"]` (the last 64 frames), plus `ctx.perf:info()`.
- Text (§7): `font:layout`, `font:measure(text, {bold, tabular})`, and `g:text_box` with `wrap`, `overflow`, `line_gap`,
  `bold`, `tabular`, `italic`, `underline` and `strike`. Kinsoku from API 2.16, and UTF-8, RTL and byte limits.
- Drawing (§6): `g:image{src, invert}`, `g:pattern`, the colour threshold, and polygon and thick-line pixels as on
  firmware 26112001.
- Network (§14): all 13 methods in replay, record, live and on the page, and `save` to files or storage.
- `ctx.system:act("refresh")`; `act("sleep")` works from a key or pointer `up` and locks the device. Also
  `ctx.url:query`, `base64` with the url alphabet, and the `ctx.files` of §29.
- `ctx.ui` (§16), in part, as the next section describes.
- `tests/conformance/`: checks for the contract rules, grouped by section and run with `tools/conformance.py`. All of
  them pass; `GAPS.md` names what no check covers yet.
  `spec/deviations.md` lists where inkemu follows the measured reader instead of the contract.

### `ctx.ui` is partial

`ctx.capabilities.ui` is true and these parts work: `create`, `begin_frame`/`end_frame`, `label`, `button`, `region`,
`radio`, `handle_event`, `focus`/`focused`, arrow-key navigation, `row`/`column` (the 2.15 flex layout and the earlier
rules) and `ui:layout`.

These raise "not implemented in inkemu yet":

- the methods `switch`, `tabs`, `image`, `field`, `stepper`, `pager`, `list`, `progress`, `grid`, `stack`, `skin`,
  `editing` and `stop_edit`;
- the same kinds as children in a frame, and `grid` and `stack` children;
- a button `image`;
- a registered skin on a button, a radio, a `switch` or `tabs` child, or a container in a frame.

### Known limits

- The CLI (`inkemu run`, `serve` and the client verbs), the daemon, the MCP server and `inkemu window` are still
  Python on top of the Rust core, and the window is still Tk. A Rust CLI and a window built on the page are planned
  for 0.3.
- README "Differences from the device" lists what is not emulated and what was not verified on the device.

## 0.1.1

- Measure text with 1-bit hinting, so widths match the drawn frame.

## 0.1.0

- First release: a Python and `lupa` emulator of the InkApp host with a Tk window, a one-shot CLI, a daemon, an MCP
  server, virtual time and recorded network fixtures.

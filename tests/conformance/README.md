# Conformance checks

These checks pin inkemu to the InkApp API 2.x contract (the tag and section hashes are in `spec/contract.lock`).
Each one is a tiny app whose expected results are written down next to it. The runner drives the app through
`inkemu.Emu` only, with members both cores have, so the same checks run against the Rust core and the 0.1 Python
core.

## Running

```sh
uv run python tools/conformance.py              # every check
uv run python tools/conformance.py 06 12        # sections, also as 6 or §6
uv run python tools/conformance.py tests/conformance/03/color_threshold.lua
uv run python tools/conformance.py --json       # machine-readable results, with every record
uv run python tools/conformance.py --strict     # pending checks count as failures
uv run pytest -q tests/test_conformance.py      # the same checks, one pytest case each
```

Every check prints `ok`, `FAIL` (with one line per difference), `PENDING` or `XPASS`. Then each section gets a line
in the wording of the contract's conformance section: `api2 06 passed checks=8`, or `api2 06 FAILED 2 of 8`, with
` pending=3` appended when known gaps are still red. The exit code is 1 when a check fails or a pending check
passes, 2 when a check file or `pending.json` is malformed.

## Layout

- `<SS>/<name>.lua` + `<SS>/<name>.json` is one check. `SS` is the two-digit contract section (`03`, `26`) or an
  appendix letter (`A`-`D`). The check id is `<SS>/<name>`.
- A `.lua` without a sibling `.json` is a helper file that checks copy into their app through `files`.
- `conformance.lua` is the prelude every check app can `require`.
- `pending.json` maps check ids of known core gaps to a one-line reason: `{"06/pattern_kind": "returns nothing"}`.
  pytest runs them as strict xfail and the CLI reports them apart, so a gap does not turn the suite red, and a gap
  that gets fixed does (remove its entry then). An id with no check is an error.

## The app

The runner builds a fresh app in a temp directory for every check:

- `manifest.json`: `{"app_id": "conformance", "entry": "index.lua", "api": "2.33", "permissions": []}`, updated
  with the check's `manifest` object (a shallow merge).
- `index.lua`: the check's `.lua`.
- `conformance.lua`: the prelude.
- the `files` map: app path -> helper file next to the check, e.g. `{"lib/util.lua": "globals_absent.util.lua"}`.

It starts `Emu` with `fresh=True`, `net="replay"`, `firmware_api=(2, 33)`, its own state and fixtures directories
and `start_time` = the check's `start_time` or `2026-11-20T09:00:00`, then runs `steps` and evaluates. Besides the
constructor it only touches `wait`, `tap`, `key`, `answer`, `lock`, `unlock`, `restart`, `reload`, `status()`,
`shot(dialog=False)`, `log` and `errors`, which both cores have.

## The prelude

```lua
local T = require("conformance")
T.value("screen.width", ctx.screen.width)             -- the value itself
T.call("write", function() return ctx.storage:write("a", "b") end)
--   {"ok": true, "ret": [true]}, every return value in order, nil as null
--   {"ok": false, "err": "message"} when it raises; the "file:line: " prefix is cut off
T.type("now.weekday", t.weekday)                      -- "integer" or "float" for numbers, else type(v)
T.hex(bytes)                                          -- lowercase hex, without ctx.hex
```

Each record goes to the log at once as `@conformance <name> <json>`, through `ctx.log:info` when `ctx.log` is a
table and `ctx:log` otherwise. The prelude reaches `ctx` as a global: the contract only hands it to callbacks, but both
inkemu cores also set the global. Record names are one word (no
spaces); anything else raises a Lua error. The prelude is plain Lua and writes its own JSON:
sequences (including `{}`) become arrays, other tables objects with sorted string keys, functions and other host
values `{"$type": "function"}`. Strings must be valid UTF-8: a binary string shows up as
`{"invalid_utf8_hex": "..."}`, so hex-encode digests with `T.hex`. Floats are printed with `%.17g`, which shows
the core's own precision (a single-precision `0.1` is `0.10000000149011612`), so expect integers or floats that
are exact in binary, such as `0.5` or `1.25`. Do not record host tables whole (they may be proxies); record the
fields you mean.

## The JSON

```json
{
  "spec": "§9",
  "about": "what the check pins, in our words",
  "manifest": {"permissions": ["net"]},
  "files": {"lib/util.lua": "name.util.lua"},
  "fixtures": [{"method": "GET", "url": "https://x.test/a", "status": 200, "body": "hi",
                "headers": {"content-type": "text/plain"}}],
  "start_time": "2026-11-20T09:00:00",
  "steps": ["wait 1s", "tap 10 10", "key ok", "answer yes", "lock", "unlock", "restart", "reload", "observe s"],
  "expect": {
    "after.files": {"value": 1, "spec": "§9"},
    "input.keyboard_h": {"value": 436, "deviation": "D1", "spec_value": 340},
    "s.running": {"value": true, "spec": "§2"}
  },
  "errors": [],
  "log": ["substring"],
  "pixels": [[10, 10, 1], [11, 10, 0], [12, 10, 0, 2]]
}
```

- `spec` names the main section; every `expect` entry names its own `spec`, or a `deviation` id plus the
  contract's `spec_value` when inkemu follows a measured reader instead; `spec/deviations.md` lists them.
- `expect` keys are record names, or `<label>.<field>` of an `observe <label>` step, which captures `Emu.status()`
  (nested objects are flattened as well: `s.pending.kind`). A name recorded more than once keeps its last value; a
  missing record fails; records nobody expects are ignored.
- Values compare as JSON, strictly: `true` is not `1`, and `436` is not `436.0`.
- `errors`: `[]` (also the default) means no Lua error at all; otherwise every listed substring must appear in some
  error and other errors are allowed. Error texts differ between cores, so to pin that a call raises, record
  `T.value("raises", not pcall(f))` and expect `true`.
- `log`: substrings that must appear somewhere in the log.
- `pixels`: `[x, y, 1]` for ink and `[x, y, 0]` for paper on the final frame, taken with `shot(dialog=False)`. An optional
  fourth value is the gray level 0-3 (the ink steps of a gray frame): `[x, y, 0, 2]`. It is checked only when the shot is
  a gray frame. The proposed way to tell: `Emu.shot()` returns a Pillow image of mode `"L"` for a gray frame, with the
  values 0, 85, 170 and 255 for levels 3, 2, 1 and 0, and mode `"1"` for every other frame. The ink value is always checked
  (`1` only for level 3), so it must agree with the level: `[x, y, 1, 3]`, `[x, y, 0, 2]`. See `GAPS.md`, section 3.
- `fixtures`: replayed network answers, written in the 0.1 fixture format (`net/METHOD_host_hash.json` plus a
  `.body` file). Optional fields: `body_hex` for binary bodies, `body_repeat` (`["x", 131082]`) for a large body,
  `delay_ms`, `match` (`exact`, `prefix`, `glob`) and `request_body` for a POST matched by its body.
  The core serves the first fixture that matches a request, so two fixtures for the same request (same method,
  URL, body and `match`) are refused as a malformed check.
- Steps: `wait <n>[ms|s|m|h]`, `tap <x> <y>`, `key up|down|left|right|ok|back`, `answer yes|no`, `lock`, `unlock`,
  `restart`, `reload`, `observe <label>`.
- Lock screen: put the entry script in a helper file next to the check, map it with `files`
  (`{"lockscreen.lua": "name.lockscreen.lua"}`) and add the manifest's `lockscreen` block (`entry`, `interval_sec`, `net`,
  `net_interval_sec`). The helper can `require("conformance")` like the app; its records reach the log. Sessions share
  the app's storage, so a helper can count sessions there. Waits are virtual time, so `lock` + `wait 3601s` is cheap.

## Adding a check

1. Read the contract section and pick one observable rule with one expected result.
2. Write `<SS>/<name>.lua` and `<SS>/<name>.json`; the `about` line and any comments are in our own words, never
   quoted from the contract.
3. Run it. If inkemu gets it wrong, add the id to `pending.json` with the reason, so the gap is tracked rather than
   hidden.

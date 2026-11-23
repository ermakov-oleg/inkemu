# Deviations from the API contract

inkemu is checked against the official InkApp API contract, locked at `api-2.33.1` (see `spec/contract.lock`;
`tools/spec_sync.py` compares the lock with upstream). The conformance checks in `tests/conformance/` pin the rules they cover; `tests/conformance/GAPS.md` lists what is pending or not covered.

Rule: the reader beats the contract only when the behaviour was measured on firmware 26112001. Then inkemu follows
the reader, the check names the deviation id and carries the contract's value as `spec_value`, so a green run never
hides it. When the reader was not measured, inkemu follows the contract; if an older firmware contradicted it, that is
noted as unverified on 26112001.

"Probe" below means a probe app run on the real firmware 26112001 in a QEMU emulator (2026-11-22). That emulator has
no radio and is not the device, so panel timing and network completion were not measured.

## Deviations

### D1 - keyboard height

- Contract says: the x4_pro profile (§3) lists a docked keyboard height of 340.
- Reader does: `ctx.input.keyboard_h` is 436 (firmware 26112001 inventory and the probe).
- inkemu does: 436.
- Status: measured on 26112001.
- Checks: `03/device_profile` (`input.keyboard_h`).

### D2 - polyline and polygon over 128 points

- Contract says: a wrong point count is `nil, "invalid_argument"` (§6).
- Reader does: more than 128 points returns `nil, "limit_exceeded"`; too few points and odd coordinate arrays are
  `invalid_argument`, as in the contract (probe).
- inkemu target: follows the reader for 129 or more points (`nil, "limit_exceeded"`). The core does not follow yet:
  `06/poly_over_128` is pending (see `tests/conformance/GAPS.md`).
- Status: measured on 26112001.
- Checks: `06/poly_over_128`.

### D3 - emphasis button below API 2.14

- Contract says: before 2.14 an `emphasis` button is drawn with a 3 px stroke and filled only when focus is drawn;
  2.14 changed it to a filled box with text in 0 (§16).
- Reader does: the filled look for every package, 2.12 and 2.13 included (the reader's UI library from 26112001, run
  as a black box with our scenarios).
- inkemu does: the filled look for every package.
- Status: measured on 26112001.
- Checks: `16/button_emphasis_before_2_14`.

## Notes

### N1 - completion of PUT, PATCH and DELETE

- Contract says: the 13 listed methods are accepted and their completion reaches the callback (§14).
- Reader does: the three methods are accepted synchronously (a job is returned). Whether the callback completes was not
  measured: the probe has no radio, and an older firmware (26102401) hung on them.
- inkemu target: follows the contract. The core does not follow yet: `14/net_methods_accepted` is pending (see
  `tests/conformance/GAPS.md`).
- Status: unverified on 26112001.
- Checks: `14/net_methods_accepted`.

## Measured and equal to the contract

Earlier notes got these wrong; do not reintroduce them.

- `ctx.hash:crc32` returns a signed 32-bit integer (for example -390611389 for `"a"`). The old "float" reading came from
  printing it with `%.1f`, which passes it through a single-precision float. Check: `26/hash_crc32`.
- `sha256`, `sha1`, `md5` and `hmac_sha256` return raw bytes, not hex. Checks: `26/hash_sha1`, `26/hash_hmac_sha256`.
- The 1025th draw command of a frame returns `nil, "limit_exceeded"`.
- `g:pattern` with an unknown kind or a density of 101 returns `nil, "invalid_argument"`.
- A write of 262145 bytes to `ctx.storage` or `ctx.files` is `limit_exceeded`; bad names are `invalid_argument`.
- `ctx.random:bytes(n)` accepts 1 to 1024; 0 and 1025 are `invalid_argument`.
- `ctx.files:read` without a length returns at most 65536 bytes.
- `ctx.fonts:size(7)` and `size(97)` are `invalid_argument`; `px` is nil without a user font.
- `invalidate("bogus")` and `act("bogus")` are `invalid_argument`.
- Net methods are upper case only; `save` is accepted; a second request while one runs is `busy`; a bad method is
  refused before `busy` is considered.
- Lua numbers are 32-bit integers and single-precision floats.

## Not emulated

BLE (§31), home screen widgets (§33), native inference `ctx.nn` (§30), `ctx.crypto`, user TTF fonts, and the battery
and Wi-Fi rules of the lock screen (§15). These are nil or ignored in inkemu. The UI library (§16) is in progress; see
the README.

## inkemu choices where the contract is silent

- The first frame of a session is a full refresh (the screen is blank before it); check `05/refresh_first_frame`.
- Where the contract names no error code, the checks infer one; `tests/conformance/GAPS.md` marks each such case.
- `sleep` and `power_off` of `ctx.system:act` were not measured on firmware 26112001; inkemu follows the contract
  (see `tests/conformance/GAPS.md`).
- Unfixtured replay requests answer `no_fixture`, which the contract does not know.
- Time stands still for remote commands so that screenshots are stable.
- `ctx.ui`, as measured with the reader's UI library run as a black box: a control declared with no open frame
  returns `nil, "invalid_argument"`; `begin_frame` while a frame is open is `busy` whatever its argument; an error
  raised by a region's `draw` still closes the frame. Checks: `16/frame_pairing`, `16/region_draw_args`.
- `ctx.ui`, not measured: `begin_frame` takes as a canvas any table with the canvas's `rect` and `text_box` methods,
  since Lua cannot tell host objects apart.

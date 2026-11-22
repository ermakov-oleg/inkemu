# Core gaps against API 2.33

Every entry below is a rule of the API 2.33 contract that the Rust core does not follow yet. Each one is pinned by a
conformance check listed in `pending.json`; when the core is fixed the check turns green and its pending entry goes.
Source: `probe-confirmed` means the behaviour was measured on the reader (firmware 26112001); `contract only` means the
contract states it and the reader was not measured. File names are relative to the repository root.

## §5 Presenting a frame

### 05/cmd_cap_1025, 05/cmd_cap_clear
- Contract: a frame holds 1024 commands; the call that would be command 1025 (and `g:clear` past the cap) returns `nil, "limit_exceeded"` and is not recorded; earlier commands stay.
- Core now: every call returns nothing; commands past the cap are silently dropped at record time and a warning is logged.
- Touch points: `crates/inkcore/src/draw/mod.rs` (method wrapper), `crates/inkcore/src/draw/canvas.rs` (`record`).
- Source: probe-confirmed.

### 05/cmd_cap_text_box_lines
- Contract: each line `g:text_box` draws is one command; a call that would pass the cap is refused whole and does not count.
- Core now: a `text_box` is one command whatever its line count, and nothing is refused.
- Touch points: `crates/inkcore/src/draw/canvas.rs` (`record`, `layout`), `crates/inkcore/src/draw/mod.rs`.
- Source: contract only.

### 05/cmd_cap_underline
- Contract: in packages of 2.28 and later an underline or strikethrough line counts as one extra command.
- Core now: not counted, since the styles are not drawn.
- Touch points: `crates/inkcore/src/draw/canvas.rs`, `crates/inkcore/src/draw/text.rs`.
- Source: contract only.

### 05/invalidate_args
- Contract: `ctx:invalidate()` and `ctx:invalidate("full")` return nothing; any other argument returns `nil, "invalid_argument"`.
- Core now: every argument is accepted.
- Touch points: `crates/inkcore/src/ctx/mod.rs` (`invalidate`).
- Source: probe-confirmed for `"bogus"`; the number case is contract only.

### 05/perf_info
- Contract: `ctx.perf:info()` returns integer fields `tick_us`, `draw_us`, `cmd_count`, `image_count`, `asset_hit`, `asset_miss`, `draw_frame`; `cmd_count` is the commands accepted in the latest frame.
- Core now: an empty table.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (perf), `crates/inkcore/src/draw/canvas.rs` (counter).
- Source: contract only.

### Refresh kind: proposed interface
The refresh kind is not observable today. Proposed: two fields. `Emu.status()["refresh"]` is the kind of the last presented app frame and `Emu.status()["refreshes"]` is the list of kinds of all app frames presented this session, oldest first (a single `tap` or `wait` step can present more than one frame, so a check that needs a middle frame reads the list). A kind is one of `"partial"`, `"full"`, `"gray"` (`"gray"` is the full refresh with the grayscale waveform of a frame that has a `g:image` with `gray = true`). The runner reads it through `observe`, so no runner change is needed. The contract does not classify the first frame of a session; inkemu chooses `"full"` for it (see below). The automatic-full checks assume that any full frame resets the partial counter.
- Touch points: `crates/inkcore/src/emu.rs` (frame presentation, status), `crates/inkemu-py/src/lib.rs` and `src/inkemu/emu.py` (`status`).

### 05/refresh_first_frame (inkemu choice)
- Contract: silent about the first frame of a session.
- inkemu choice: the first frame is `"full"` (the screen is blank before it). Not a contract rule; change the check if the choice changes.
- Core now: no refresh kind is recorded.
- Touch points: `crates/inkcore/src/emu.rs`.
- Source: inkemu choice.

### 05/refresh_plain, 05/refresh_full_arg, 05/refresh_full_wins, 05/refresh_full_in_draw
- Contract: a plain `invalidate()` presents a partial refresh and `invalidate("full")` a full one; if a frame is requested several times, one `"full"` request makes the whole frame full; a request made from `on_draw` (including `"full"`) applies to the next frame. `05/refresh_full_in_draw` expects `refreshes == ["full", "partial", "full"]` (session start, the frame that asks, the deferred frame) and `refresh == "full"`.
- Core now: the argument is ignored and the kind is not recorded.
- Touch points: `crates/inkcore/src/ctx/mod.rs` (`invalidate`), `crates/inkcore/src/emu.rs` (frame scheduling).
- Source: contract only.

### 05/refresh_auto_full
- Contract: the system makes a full refresh after a set number of partial ones; the default is 30, so after 29 partial frames the 30th is full.
- Core now: every frame is the same kind; nothing counts partial frames.
- Touch points: `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 05/refresh_defer_blocks_auto
- Contract: `defer_full_refresh(ms)` stops the automatic full refresh from happening during the next `ms` milliseconds (the count keeps growing; a forced full comes at twice the interval, not tested here).
- Core now: the call is accepted and does nothing.
- Touch points: `crates/inkcore/src/ctx/basic.rs`, `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 05/defer_args
- Contract: `defer_full_refresh(ms)` takes an integer from 1 to 60000 and returns nothing; any other value returns `nil, "invalid_argument"`.
- Core now: any number is accepted and one `nil` is returned.
- Touch points: `crates/inkcore/src/ctx/basic.rs` (`defer_full_refresh`).
- Source: contract only.

## §6 Drawing

### 06/g_methods
- Contract: `g` has exactly `size clear line rect circle arc polyline polygon pattern text text_box image`.
- Core now: also exposes `round_rect`, `measure`, `measure_text`, `fill_pattern`, `pattern_rect` (0.1 leftovers).
- Touch points: `crates/inkcore/src/draw/mod.rs` (`SPECS`, `new_g`).
- Source: probe-confirmed.

### 06/canvas_inactive
- Contract: `g` is valid only during its callback; afterwards every call returns `nil, "inactive"`.
- Core now: a stored `g` keeps recording.
- Touch points: `crates/inkcore/src/draw/mod.rs`, `crates/inkcore/src/emu.rs` (frame end), `crates/inkcore/src/ctx/layers.rs`.
- Source: contract only.

### 06/polyline_points, 06/polygon_points
- Contract: `polyline` takes 2 to 128 points, `polygon` 3 to 128; a wrong count or an odd array is `nil, "invalid_argument"`.
- Core now: too few points draw nothing and return nothing; an odd array makes the frame fail with a Lua error.
- Touch points: `crates/inkcore/src/draw/mod.rs` (`points`, `command`).
- Source: probe-confirmed.

### 06/poly_over_128 (deviation D2)
- Reader: more than 128 points returns `nil, "limit_exceeded"` (the contract says `invalid_argument`); inkemu follows the reader.
- Core now: draws and returns nothing.
- Touch points: `crates/inkcore/src/draw/mod.rs` (`points`).
- Source: probe-confirmed.

### 06/pattern_invalid_args
- Contract: an unknown `kind` or a `density` outside 0..100 returns `nil, "invalid_argument"` and draws nothing.
- Core now: an unknown kind is skipped silently, density is not range-checked and is drawn.
- Touch points: `crates/inkcore/src/draw/mod.rs` (`command`, "pattern"), `crates/inkcore/src/draw/pattern.rs`.
- Source: probe-confirmed (kind `bogus`, density 101); density -1 is contract only.

### 06/arc_full_turn
- Contract: when start and end differ by a whole multiple of 360 degrees nothing is drawn.
- Core now: draws a full ring.
- Touch points: `crates/inkcore/src/draw/raster.rs`.
- Source: contract only.

### 06/circle_fill_pixels
- Contract: a filled circle covers pixel centers strictly closer than `r` to `(cx + 0.5, cy + 0.5)`; a stroke is the ring of pixels at distance `r - line_width` or more inside it.
- Core now: fill includes pixels at distance exactly `r`, and a one-pixel stroke misses the outermost ring (half a pixel off).
- Touch points: `crates/inkcore/src/draw/raster.rs`.
- Source: contract only.

### 06/image_invalid_value
- Contract: `g:image` with anything but an image object returns `nil, "invalid_argument"`.
- Core now: draws nothing and returns nothing.
- Touch points: `crates/inkcore/src/draw/mod.rs` (`image_command`).
- Source: contract only.

### 06/image_layer_self
- Contract: drawing a layer into itself returns `nil, "invalid_argument"`.
- Core now: returns nothing.
- Touch points: `crates/inkcore/src/ctx/layers.rs`, `crates/inkcore/src/draw/mod.rs`.
- Source: contract only.

### 06/image_src_invalid
- Contract: `opts.src` of `g:image` is four integers (named `x y w h` or, since the 2.33 host fix, positional) with `x, y >= 0`, `w, h >= 1`, inside the image; anything else returns `nil, "invalid_argument"`.
- Core now: `src` is ignored entirely, so no error is returned (and the region is not extracted).
- Touch points: `crates/inkcore/src/draw/mod.rs` (`image_command`), `crates/inkcore/src/draw/canvas.rs`.
- Source: contract only.

### 06/image_released
- Contract: `layer:release()` returns true (repeatedly); afterwards `layer:draw` and any `g:image` using the layer return `nil, "invalid_handle"`.
- Core now: `release` returns nothing, `layer:draw` raises a Lua error and `g:image` fails the whole frame.
- Touch points: `crates/inkcore/src/ctx/image.rs`, `crates/inkcore/src/ctx/layers.rs`, `crates/inkcore/src/draw/mod.rs` (`image_command`).
- Source: contract only.

## §7 Text and fonts

### 07/text_invalid_utf8, 07/text_rtl
- Contract: text that is not valid UTF-8, or that holds a right-to-left character, returns `nil, "invalid_argument"` from `g:text` and `g:text_box`; invalid UTF-8 is also `invalid_argument` for `font:measure` and `font:layout`.
- Core now: invalid bytes are converted lossily and drawn; right-to-left text is skipped with a warning; both return nothing.
- Touch points: `crates/inkcore/src/draw/mod.rs` (`text_of`, `command`), `crates/inkcore/src/draw/canvas.rs` (`layout`), `crates/inkcore/src/ctx/fonts.rs`.
- Source: contract only.

### 07/text_limits_call, 07/text_limit_frame, 07/layout_limit
- Contract: `g:text`, `g:text_box`, `font:measure` take at most 8192 bytes per call, all text in a frame at most 65536 bytes, `font:layout` at most 65536 bytes per call; beyond that `nil, "limit_exceeded"`.
- Core now: no limits; `font:layout` does not exist.
- Touch points: `crates/inkcore/src/draw/mod.rs`, `crates/inkcore/src/draw/canvas.rs`, `crates/inkcore/src/ctx/fonts.rs`.
- Source: contract only.

### 07/measure_bold
- Contract: `font:measure(text, { bold = true })` is the plain width plus 1 (empty text stays 0); italic, underline and strike do not change the width.
- Core now: `measure` ignores its options.
- Touch points: `crates/inkcore/src/ctx/fonts.rs`, `crates/inkcore/src/draw/text.rs`.
- Source: contract only.

### 07/measure_control_chars
- Contract: a tab measures as one space; `\n` and other control characters have width 0.
- Core now: they are measured as glyphs of the bundled font.
- Touch points: `crates/inkcore/src/draw/text.rs`.
- Source: contract only.

### 07/layout_results, 07/layout_paging, 07/layout_matches_text_box, 07/font_object
- Contract: `font:layout(text, w, h, opts)` returns `lines, height, consumed` as integers (height `n * line_height + (n - 1) * line_gap`, `consumed` on a code-point boundary including a trailing newline), follows the `text_box` rules, and its line count equals the commands `text_box` takes.
- Core now: font objects have no `layout` method.
- Touch points: `crates/inkcore/src/ctx/fonts.rs`, `crates/inkcore/src/draw/canvas.rs` (`wrap`, `layout`).
- Source: probe-confirmed that `layout` exists and returns three integers; the rules are contract only.

### 07/fonts_size_range, 07/fonts_size_sample
- Contract: `ctx.fonts:size(px, sample)` takes an integer `px` from 8 to 96, else `nil, "invalid_argument"`; `sample` is nil or text, and invalid UTF-8, NUL or right-to-left text is `invalid_argument`; a value that is neither string nor nil raises a Lua error.
- Core now: any `px` is accepted and `sample` is ignored.
- Touch points: `crates/inkcore/src/ctx/fonts.rs` (`size`).
- Source: probe-confirmed for 7 and 97; the rest is contract only.

### 07/font_metrics
- Contract and reader: system font line heights 20, 24, 30 and ascents 17, 21, 26 on x4_pro.
- Core now: line heights match, ascents come from the bundled font (16, 19, 23).
- Touch points: `crates/inkcore/src/draw/text.rs` (`ascent`).
- Source: probe-confirmed for small (17) and medium (21); large (26) is contract only.

## §25 System actions

### 25/act_refresh
- Contract: `ctx.system:act("refresh")` returns nothing and makes the next frame a full refresh, like `invalidate("full")`; it is allowed in any callback.
- Core now: every name returns `nil, "invalid_argument"`.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs`.
- Source: probe-confirmed (returns nothing); the full frame is contract only.

### 25/act_inactive
- Contract: `act("sleep")` and `act("power_off")` return `nil, "inactive"` outside an `on_input` delivering a key up or a pointer up (load, timers, pointer down).
- Core now: `nil, "invalid_argument"`.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs` (input dispatch must flag an "up" event in progress).
- Source: contract only (`sleep` and `power_off` were not called in the probe).

### 25/act_sleep_tap, 25/act_sleep_key
- Contract: `act("sleep")` from a key up or pointer up returns nothing, ends the session after the callback (calling `on_leave`) and locks the device, like a short power-key press.
- Core now: the call returns `nil, "invalid_argument"` (logged as not emulated); there is no `on_leave` and no lock.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs` (lock path).
- Source: contract only.

Already green and pinned: `25/act_args` (unknown name is `nil, "invalid_argument"` - probe-confirmed; a non-string name raises). Not covered: `act("power_off")` with its confirmation dialog and `busy` (contract only, needs system dialog support).

## Deviations

Deviations these checks rely on (no new ids; D1 and D2 come from the constraints):

- D2: `g:polyline` and `g:polygon` with more than 128 points return `nil, "limit_exceeded"` on the reader, the contract says `invalid_argument`. Check: `06/poly_over_128`. Source: probe-confirmed.

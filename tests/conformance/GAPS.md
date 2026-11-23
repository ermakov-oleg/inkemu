# Core gaps against API 2.33

Every entry below is a rule of the API 2.33 contract that the Rust core does not follow yet. Each one is pinned by a
conformance check listed in `pending.json`; when the core is fixed the check turns green and its pending entry goes.
Source: `probe-confirmed` means the behaviour was measured on the reader (firmware 26112001); `contract only` means the
contract states it and the reader was not measured. File names are relative to the repository root.

## §1 Globals

Already green and pinned: `01/globals_absent` (the globals that must not exist).

## §3 Gray frames

### Gray frames: proposed interface
A frame is gray when one of its `g:image` calls sets `gray = true` (outside `layer:draw`; `ctx.screen.gray_still` is 4, probe-confirmed). `Emu.status()["refresh"]` and `["refreshes"]` report such a frame as `"gray"` (see "Refresh kind: proposed interface" under §5). `Emu.shot()` returns a Pillow image of mode `"L"` for a gray frame, with the gray levels 0, 85, 170 and 255 for 3, 2, 1 and 0 ink steps, and keeps returning mode `"1"` for every other frame. The runner tells the two apart by `image.mode`: a `pixels` entry `[x, y, ink, level]` always checks ink (`1` only for black, the 3-step level) and also checks the level 0-3 when the shot is gray, so the level is ignored on a 1-bit frame. The core stays 1-bit for normal frames; only a gray frame needs the 4-step buffer.
- Touch points: `crates/inkcore/src/draw/canvas.rs` and `crates/inkcore/src/draw/image.rs` (frame buffer and `g:image` options), `crates/inkcore/src/emu.rs` (frame presentation, `frame`), `crates/inkemu-py/src/lib.rs` and `src/inkemu/emu.py` (`_image`, `shot`).

### 03/gray_frame_refresh, 03/gray_next_frame_full, 03/gray_layer_ignored
- Contract: `gray = true` on a `g:image` makes the frame a gray one, shown with the full-refresh gray waveform; the next ordinary frame is full again and the one after goes back to partial; `gray = true` inside `layer:draw` is ignored.
- Core now: `gray` is ignored and no refresh kind is recorded.
- Touch points: `crates/inkcore/src/draw/image.rs`, `crates/inkcore/src/ctx/layers.rs`, `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 03/gray_pixels
- Contract: every color of a gray frame is scaled to `gray_still` steps as `(color * gray_still) // 16`, and a 1-bit image keeps only black and white. The check expects colors 3, 4, 8, 12 and 15 to come out as 0, 1, 2, 3 and 3 steps.
- Core now: the shot is always 1-bit, so a rectangle of color 8 or more is full ink and the levels cannot be told apart.
- Touch points: as for the interface above.
- Source: contract only.

Already green and pinned: `03/color_threshold`, `03/device_profile` (the profile values, with the keyboard height as deviation D1).

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
- Contract: from API 2.28 on, each underline or strikethrough stroke costs one more command of the 1024.
- Core now: not counted, since the styles are not drawn.
- Touch points: `crates/inkcore/src/draw/canvas.rs`, `crates/inkcore/src/draw/text.rs`.
- Source: contract only.

### 05/invalidate_args
- Contract: `ctx:invalidate()` and `ctx:invalidate("full")` return nothing; every other argument gives `nil, "invalid_argument"`.
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
- Contract: the host inserts a full refresh every N frames (default 30), so 29 partial frames are followed by a full one.
- Core now: every frame is the same kind; nothing counts partial frames.
- Touch points: `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 05/refresh_defer_blocks_auto
- Contract: `defer_full_refresh(ms)` stops the automatic full refresh from happening during the next `ms` milliseconds (the count keeps growing; a forced full comes at twice the interval, not tested here).
- Core now: the call is accepted and does nothing.
- Touch points: `crates/inkcore/src/ctx/basic.rs`, `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 05/defer_args
- Contract: `defer_full_refresh(ms)` takes an integer 1..60000 and returns nothing; anything outside that is `nil, "invalid_argument"`.
- Core now: any number is accepted and one `nil` is returned.
- Touch points: `crates/inkcore/src/ctx/basic.rs` (`defer_full_refresh`).
- Source: contract only.

Already green and pinned (the valid cases of the rules above): `05/cmd_cap_text_box_counter` (the rectangles that still fit after a refused `text_box`), `05/cmd_cap_underline_text`, `05/invalidate_valid`.

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
- Contract: `g:image` with the layer being drawn as its own source is `nil, "invalid_argument"`.
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

Already green and pinned (the valid cases of the rules above): `06/g_methods_present`, `06/polyline_points_valid`, `06/polygon_points_valid`, `06/pattern_valid_args`, `06/image_src_valid`, `06/arc_quadrant`, `06/pattern_unknown_kind`, `06/polygon_fill_stroke`, `06/rect_radius_rows`, `06/rect_stroke_inward`, `06/rect_zero_size`, `06/success_returns_nothing`.

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

### Performance: text_box wrapping of a long unbreakable word
- Not a contract rule. The core wraps one unbreakable word of 8192 bytes in a 100 by 100 box in quadratic time (about 150 s). `07/text_limits_call` therefore uses breakable text for its `text_box` cases; the rule it pins is unchanged.
- Touch points: `crates/inkcore/src/draw/canvas.rs` (`wrap`, `layout`).
- Source: measured on this core.

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

Already green and pinned (the valid cases of the rules above): `07/font_line_heights`, `07/font_object_fields`, `07/fonts_size_valid`, `07/fonts_size_sample_valid`, `07/measure_other_opts`, `07/text_utf8_valid`, `07/text_latin`, `07/text_limit_frame_fit`, `07/text_limits_call_max`, `07/fonts_size_tier`.

## §9 Private files

### 09/storage_name_invalid
- Contract: a name is 1 to 64 bytes from `[a-z0-9._-]` and does not start or end with `.`; any other name returns `nil, "invalid_argument"` from `write`, `read`, `size` and `remove`, and `exists` returns false.
- Core now: `valid_name` is the 0.1 rule (any UTF-8 up to 255 bytes, no whitespace or slashes), so a capital, a leading or trailing dot and a 65-byte name are stored. Empty names, spaces and slashes are already refused; `09/storage_name_refused` pins them (green).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`valid_name`; storage and files need separate rules).
- Source: probe-confirmed for `write` and `remove` with `Bad`, `.x` and 65 bytes, and for `exists('Bad')`; the other names and calls (`read`, `size`) are contract only.

### 09/storage_file_size_limit
- Contract: a `write` of more than 262144 bytes returns `nil, "limit_exceeded"` and an existing file stays as it was.
- Core now: no per-file limit, only the total quota. A file of exactly 262144 bytes works (`09/storage_file_size_max`, green).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`writer`).
- Source: probe-confirmed for the 262145-byte write on a new name; the unchanged old file is contract only.

### 09/storage_file_count_limit
- Contract: a `write` that would create a 513th file returns `nil, "limit_exceeded"`; rewriting or removing keeps working.
- Core now: the same refusal, but the code is `no_space`.
- Touch points: `crates/inkcore/src/ctx/files.rs` (`writer`).
- Source: contract only.

### 09/storage_remove_missing
- Contract: `remove` returns true for a file that does not exist.
- Core now: `nil, "not_found"`. Removing an existing file is pinned apart (`09/storage_remove_existing`, `09/storage_name_exists_invalid`, green).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`remove`).
- Source: probe-confirmed.

Already green and pinned: `09/storage_usage`, `09/storage_name_valid`, `09/storage_name_refused`, `09/storage_file_size_max`, `09/storage_remove_existing`, `09/storage_missing_file`, `09/storage_write_replaces`, `09/storage_list_order` (byte order is contract only; the probe sample was alphabetical), `09/storage_full_quota` (512 full-size files are exactly the 128 MB total, so the total quota is not reachable separately and is covered by this check).

## §12 Clock, log and memory

### 12/clock_weekday_sunday, 12/clock_weekday_saturday
- Contract: `weekday` is 0 for Sunday (6 for Saturday).
- Core now: `weekday` is 1-based from Sunday (Friday 6, Sunday 1, Saturday 7), the Lua `os.date` numbering.
- Touch points: `crates/inkcore/src/ctx/clock.rs`.
- Source: contract only.

### 12/clock_revision_start
- Contract: `revision` is an integer and is 1 at the start of the session.
- Core now: 0.
- Touch points: `crates/inkcore/src/ctx/clock.rs`.
- Source: contract only.

### 12/log_methods
- Contract: `ctx.log:info`, `warn` and `error` raise a Lua error for a non-string argument (returning nothing is pinned apart in `12/log_returns_nothing`, green).
- Core now: they accept tables, nil and booleans.
- Touch points: `crates/inkcore/src/ctx/mod.rs` (log object).
- Source: contract only.

Already green and pinned: `12/log_and_clock`, `12/clock_fields` (date and time fields, epoch and local seconds), `12/clock_snapshot`, `12/log_returns_nothing`, `12/clock_millis`.

## §14 Networking

All network checks use replay fixtures on `.test` hosts, so they pass the same way on any core that answers fixtures. The current core only answers `GET` and `POST` and ignores `save`, so most of these are red. Unverified on 26112001 (no radio in the probe): completion of `PUT`, `PATCH`, `DELETE` and the rest; the checks follow the contract (note N1 in `spec/deviations.md`).

### 14/net_methods_accepted, 14/net_head_no_body
- Contract: the 13 methods `GET POST PUT PATCH DELETE HEAD OPTIONS PROPFIND PROPPATCH MKCOL COPY MOVE REPORT` are accepted; a fixture for each reaches the completion function. A `HEAD` answer has `body = ""`, `bytes = 0` and its headers. `14/net_methods_basic` pins `GET` and `POST` (green); `net_methods_accepted` the other eleven.
- Core now: any other method than `GET` and `POST` logs a warning and never calls back.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`first_step`), `crates/inkcore/src/fixtures.rs` (matching already works for any method).
- Source: probe-confirmed that `GET`, `PUT`, `PATCH` and `DELETE` return a job synchronously; the other methods and every completion are contract only.

### 14/net_method_case
- Contract: the method string must match exactly and in upper case; lower or mixed case and unknown names give `nil, "invalid_argument"`.
- Core now: the method is upper-cased, so `get`, `post`, `patch` and `put` start a request, and an unknown method starts one that never answers. An empty method is already refused (`14/net_method_empty`, green).
- Touch points: `crates/inkcore/src/ctx/net.rs` (`request`).
- Source: probe-confirmed for `patch` and `FOO`; the other spellings and the empty string are contract only.

### 14/net_body_get_head, 14/net_body_rules
- Contract: a body on `GET` or `HEAD` is `invalid_argument` (`net_body_get_head`); a body is at most 65536 bytes (`net_body_rules`); other methods may carry a body (`14/net_body_allowed`, green).
- Core now: nothing is checked.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`request`).
- Source: contract only. The error code for a body over 65536 bytes is not stated in §14; the check expects `limit_exceeded` by the general rule of §18 (contract, §18 general rule).

### 14/net_header_invalid
- Contract: headers are at most 16 entries, names at most 64 bytes, all names and values together at most 65536 bytes (2.26), and no value holds CR, LF or NUL; otherwise `nil, "invalid_argument"`.
- Core now: nothing is checked (a non-string value is dropped).
- Touch points: `crates/inkcore/src/ctx/net.rs` (`header_pairs`, `request`).
- Source: contract only. `14/net_header_limits_ok` pins the accepted edges and is green.

### 14/net_url_length
- Contract: the URL is at most 1024 bytes. The scheme rule (`http://` or `https://`) is already followed and pinned by `14/net_url_scheme` (green).
- Core now: the length is not checked.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`url_ok`).
- Source: contract only. §14 names no error code for a long URL; the check expects `limit_exceeded` (contract, §18 general rule).

### 14/net_timeout_range
- Contract: `timeout_ms` is an integer from 1000 to 60000; anything else is `invalid_argument` (the accepted ends are pinned by `14/net_timeout_accepted`, green).
- Core now: any number is accepted.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`request`, `round_ms`).
- Source: contract only.

### 14/net_busy
- Contract: only one request is unfinished at a time; another `request` returns `nil, "busy"`, but a bad method is `invalid_argument` even then; the slot frees when the answer arrives.
- Core now: no busy state, requests run side by side.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`NetState.inflight` already counts them), `crates/inkcore/src/emu.rs`.
- Source: probe-confirmed (`busy`, and `FOO` while busy is `invalid_argument`). Not pinned: the slot a cancelled request holds until the host aborts it.

### 14/net_response_fields, 14/net_response_error_status
- Contract: a complete answer, with any status, has `ok = true`, `status`, `headers` (lowercase names), `body`, `bytes`, and no `err`, `saved` only with `save`.
- Core now: the table has no `ok` (it has the 0.1 fields `code` and `error` too).
- Touch points: `crates/inkcore/src/ctx/net.rs` (`response_table`).
- Source: contract only.

### 14/net_body_cap
- Contract: at most 131072 body bytes are kept; a longer body is truncated with `err = "limit_exceeded"` and `ok` stays true; `bytes` is the full size. A body of exactly 131072 bytes is whole (`14/net_body_cap_exact`, green).
- Core now: the body is delivered whole and `err` stays nil.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`response_table`).
- Source: contract only.

### 14/net_save_files_2xx, 14/net_save_storage_2xx, 14/net_save_non_2xx
- Contract: `save = { to = "files" | "storage", name }` on a 2xx answer replaces the whole file before the callback: `saved = true`, `body = ""`, `bytes` the size, `err` nil. A non-2xx answer leaves the file alone: `saved = false`, the body in `res.body`.
- Core now: `save` is ignored; the body stays in `res.body` and `saved` is nil.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`request`, `response_table`), `crates/inkcore/src/ctx/files.rs` (a shared store function for both directories).
- Source: contract only (the probe could only see `saved = false` after `no_network`).

### 14/net_save_invalid
- Contract: `save` must be a table whose `to` is `files` or `storage` and whose name is valid for that target; otherwise `nil, "invalid_argument"`.
- Core now: not validated.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`request`), `crates/inkcore/src/ctx/files.rs` (name rules).
- Source: contract only. A missing `to` or `name` is inferred to be `invalid_argument` as well (the contract could also read it as a type error).

### 14/net_save_storage_limit, 14/net_save_files_limit, 14/net_save_files_quota
- Contract: a save to storage takes up to 262144 bytes, to the app folder up to 8388608; a longer body, or a new file past the 512-file quota, gives `ok = true`, `saved = false`, `body = ""`, `err = "limit_exceeded"` and `bytes` the full size, with no file created.
- Core now: `save` is ignored.
- Touch points: `crates/inkcore/src/ctx/net.rs`, `crates/inkcore/src/ctx/files.rs`.
- Source: contract only. These checks use the runner's `body_repeat` fixture field for the large bodies. Not pinned: the storage total-bytes quota via save, `io_error` on a full card.

### 14/net_keep_wifi
- Contract: `keep_wifi(ms)` takes an integer from -1 to 2147483647, returns nothing, and returns `nil, "invalid_argument"` for another number (-2, 1.5, 2147483648). A non-number or a missing argument raises (`14/net_keep_wifi_types`, green).
- Core now: it returns one `nil` and accepts every number.
- Touch points: `crates/inkcore/src/ctx/net.rs` (`keep_wifi`).
- Source: contract only.

Already green and pinned: `14/net_methods_basic`, `14/net_method_empty`, `14/net_body_allowed`, `14/net_url_scheme`, `14/net_timeout_accepted`, `14/net_keep_wifi_types`, `14/net_body_cap_exact`, `14/net_header_limits_ok`, `14/net_redirect_not_followed`. Not covered: `ctx.net:websocket` (not emulated), `no_network`, `timeout`, `io_error` and `out_of_memory` answers (a fixture cannot express them), unfixtured requests (replay answers `no_fixture`, which the contract does not know), the `content-type` default, and header joining of duplicates.

## §15 Lock screen

Lock-screen frames count as app frames in `status()["refresh"]` and `status()["refreshes"]` (see "Refresh kind: proposed interface" under §5): after `lock` the list has the session-start frame of the app first, then the frames of the first session, then one per timer wake. The checks below assume that. Frame counts of a networked session follow the contract's step list (one frame at once with the stored data when requests or timers remain after `on_load`, one final frame).

### 15/lock_reason_first
- Contract: `ctx.lock.reason` is `"first"` in the first session after locking and `"timer"` in a wake.
- Core now: the first session reports `"lock"`; the timer value is right (`15/lock_reason_timer`, green).
- Touch points: `crates/inkcore/src/lockscreen.rs` (`lock`, `enter`), `crates/inkcore/src/ctx/lock.rs`.
- Source: contract only.

### 15/lock_timer_alignment
- Contract: wakes are aligned to wall-clock multiples of `interval_sec`, so a 60 s interval wakes at :00 of each minute. The check locks at 09:00:20 and expects the wake at 09:01:00 with `elapsed_sec` 40.
- Core now: the wake comes `interval_sec` after locking (09:01:20).
- Touch points: `crates/inkcore/src/lockscreen.rs` (`schedule`).
- Source: contract only (not in the task list; the guide and §15 both state it).

### 15/lock_net_interval_blocks, 15/lock_net_interval_clamp, 15/lock_net_interval_clamp_low
- Contract: `lockscreen.net_interval_sec` defaults to `interval_sec` and is limited to 30-3600. `ctx.lock.net` is true in the first session; later it turns true again only after `net_interval_sec` seconds since the previous networked session, and `ctx.net` is nil while it is false. The clamp check uses `interval_sec` 600 and `net_interval_sec` 100000 and expects the sequence true, five times false, true (the wake of 3600 s). The low bound is pinned by `15/lock_net_interval_clamp_low`: locked at 09:00:50 the first wake comes 10 s later, and `net_interval_sec` 10 is raised to 30, so that wake has no network (it also depends on the alignment rule).
- Core now: `net_interval_sec` is not read; `lock.net` follows the manifest's `net` flag in every session. Default, elapsed interval and the undeclared case are pinned in `15/lock_net_interval_default`, `15/lock_net_interval_elapsed`, `15/lock_net_first`, `15/lock_net_undeclared` (green).
- Touch points: `crates/inkcore/src/lockscreen.rs` (`enter`, `LockCtx`), `crates/inkcore/src/ctx/lock.rs`, `crates/inkcore/src/ctx/net.rs` (`ctx.net` must follow `lock.net`).
- Source: contract only. Not modelled: battery below 20% and the three failed Wi-Fi connections.

### 15/lock_prep_first, 15/lock_first_frames
- Contract: in a networked first session, when requests are unfinished after `on_load`, `on_draw` runs once at once with the stored data (full refresh) and again after the preparation phase with the fetched data (partial refresh).
- Core now: the first session draws once at once and never again when the request finishes. A timer session already waits for its request and draws once (`15/lock_prep_timer`, green).
- Touch points: `crates/inkcore/src/lockscreen.rs` (`after_load`, `present`), `crates/inkcore/src/emu.rs` (frame scheduling).
- Source: contract only.

### 15/lock_prep_cap
- Contract: the preparation phase ends 28000 ms after the session start; unfinished requests and timers are cancelled and `on_draw` runs (a request that takes 29 s never delivers). `lock:done()` also ends it early (`15/lock_done_early`, green).
- Core now: a timer wake waits 30 s (`LOCK_WAIT_MS`) and a 29 s request is delivered before the draw.
- Touch points: `crates/inkcore/src/lockscreen.rs` (`LOCK_WAIT_MS`, `after_load`), `crates/inkcore/src/ctx/net.rs` (cancel on the end of the phase).
- Source: contract only.

### 15/lock_invalidate_full_prep, 15/lock_invalidate_plain_prep, 15/lock_refresh_kinds
- Contract: the first session presents a full refresh and a timer session a partial one; `ctx:invalidate("full")` during the preparation phase makes the frame of step 5 full and a plain `invalidate()` draws no extra frame.
- Core now: no refresh kind is recorded.
- Touch points: `crates/inkcore/src/lockscreen.rs`, `crates/inkcore/src/ctx/mod.rs` (`invalidate`), `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 15/lock_gray_then_normal
- Contract: a lock session may show a gray frame in any session; it is a full refresh, and the next ordinary frame is full as well, also across a timer wake. Expected kinds after lock and one wake: full (app start), gray, full.
- Core now: nothing is gray and no kind is recorded.
- Touch points: as for the gray frames in §3, plus `crates/inkcore/src/lockscreen.rs`.
- Source: contract only.

### 15/lock_after_noop
- Contract: in a lock session without `lockscreen.net`, `ctx:after` has no effect (its callback never runs). With `lockscreen.net` timers work during the preparation phase.
- Core now: the callback runs.
- Touch points: `crates/inkcore/src/ctx/mod.rs` (`after`), `crates/inkcore/src/lockscreen.rs`.
- Source: contract only.

### 15/lock_budget_inactive
- Contract: `ctx.budget:extend()` returns `nil, "inactive"` in a lock session.
- Core now: returns `true`.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`budget`).
- Source: contract only.

### 15/lock_set_interval_args
- Contract: `set_interval` takes an integer from 30 to 3600 and returns nothing. The contract names no result for other values; the check expects `nil, "invalid_argument"` like the other integer arguments (29, 3601 and 45.5).
- Core now: any positive number is accepted and `true` is returned; only 0 and negatives are refused.
- Touch points: `crates/inkcore/src/ctx/lock.rs` (`set_interval`).
- Source: contract only. The error code for a bad value is inferred, confirm it before the fix.

Already green and pinned: `15/lock_reason_timer`, `15/lock_elapsed_first`, `15/lock_net_first`, `15/lock_net_undeclared`, `15/lock_net_interval_default`, `15/lock_net_interval_elapsed`, `15/lock_prep_timer`, `15/lock_done_early`, `15/lock_set_interval_effect`. The prelude records reach `emu.log` from a lock session (these checks read them). Not covered: the system lock screen on a missing entry, error or timeout in the script (default picture, no more wakes), low battery, `ctx.lockscreen` wallpaper calls and the confirmation dialog.

## §25 System actions

### 25/act_refresh
- Contract: `ctx.system:act("refresh")` returns nothing and makes the next frame a full refresh, like `invalidate("full")`; it is allowed in any callback.
- Core now: every name returns `nil, "invalid_argument"`.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs`.
- Source: probe-confirmed (returns nothing); the full frame is contract only.

### 25/act_inactive
- Contract: `act("sleep")` and `act("power_off")` return `nil, "inactive"` anywhere except inside `on_input` for a key release or pointer release (not in load, timers or pointer press).
- Core now: `nil, "invalid_argument"`.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs` (input dispatch must flag an "up" event in progress).
- Source: contract only (`sleep` and `power_off` were not called in the probe).

### 25/act_sleep_tap, 25/act_sleep_key
- Contract: `act("sleep")` from a key up or pointer up returns nothing; once the callback ends the session is closed (`on_leave` runs) and the device locks, as with a short power-key press.
- Core now: the call returns `nil, "invalid_argument"` (logged as not emulated); there is no `on_leave` and no lock.
- Touch points: `crates/inkcore/src/ctx/stubs.rs` (`act`), `crates/inkcore/src/emu.rs` (lock path).
- Source: contract only.

Already green and pinned: `25/act_inactive_no_lock` (refused `sleep`/`power_off` calls do not lock the device), `25/act_args` (unknown name is `nil, "invalid_argument"` - probe-confirmed; a non-string name raises). Not covered: `act("power_off")` with its confirmation dialog and `busy` (contract only, needs system dialog support).

## §26 Encoding, hashes, random

### 26/hash_sha1, 26/hash_input_limit
- Contract: `sha1` returns 20 raw bytes; every hash call takes at most 1048576 bytes, more is `nil, "limit_exceeded"` (`hash_input_limit`; the accepted maximum is `26/hash_input_max`, green).
- Core now: `sha1` returns 40 hex characters; no input limit anywhere.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: probe-confirmed for `sha1` (`abc` gives 20 bytes); the limit is contract only (§26 gives it for all input; §18 gives the code).

### 26/hash_crc32
- Contract: `crc32` is zlib's value as a signed 32-bit integer; `crc32("a") == -390611389`.
- Core now: an unsigned single-precision float (`3904355840.0`).
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: probe-confirmed for `a` and `abc`; `""` and `123456789` are computed from zlib.

### 26/encoding_input_limit
- Contract: `hex:encode`, `base64:encode` and `url:encode` take at most 1048576 bytes of input; more is `nil, "limit_exceeded"` (the accepted maximum is `26/encoding_input_max`, green).
- Core now: no limit.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: contract only.

### 26/base64_url_alphabet, 26/base64_decode_url_unpadded
- Contract: `encode(s, { url = true })` uses `-` and `_` without padding; `decode` also accepts the url alphabet and missing padding. (Decoding the standard alphabet and the illegal-input errors are `26/base64_decode`, green.)
- Core now: `opts` is ignored on encode; `decode` rejects the url alphabet and unpadded input.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: contract only.

### 26/random_bytes
- Contract: `bytes(n)` takes an integer from 1 to 1024; 0 and 1025 are `nil, "invalid_argument"` (1, 1024 and a negative count are `26/random_bytes_range`, `26/base64_encode_std`, green).
- Core now: 0 returns an empty string and 1025 returns bytes.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: probe-confirmed.

### 26/url_decode_invalid
- Contract: a malformed `%XX` escape in `url:decode` is `invalid_argument` (§26: an illegal character or length when decoding). The escape and keep-`+` rules are `26/url_encode_decode` (green).
- Core now: a malformed escape is passed through unchanged.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: contract only.

### 26/url_query
- Contract: `ctx.url:query(t)` joins `key=value` items in byte order of the keys, encoding both, booleans and numbers allowed; an empty table gives `""`.
- Core now: the method does not exist.
- Touch points: `crates/inkcore/src/ctx/utils.rs`.
- Source: contract only. Inferred, not stated: booleans appear as the text `true` and `false`. Not pinned: the 64-item cap (the contract does not name the error code).

Already green and pinned: `26/encoding`, `26/hash_input_max`, `26/encoding_input_max`, `26/base64_decode`, `26/url_encode_decode`, `26/random_bytes_range`, `26/hash_hmac_sha256` (the probe's `k`/`m` vector, RFC 4231 cases 2 and 6), `26/hex_decode`, `26/random_uuid`.

## §29 App files

### 29/files_name_illegal, 29/files_name_illegal_calls, 29/files_name_inner_space
- Contract: a name is 1 to 128 UTF-8 bytes without control characters or any of `/ \ : * ? " < > | ~`, not starting with `.`, not ending with `.` or a space, and not a device name (`CON`, `PRN`, `AUX`, `NUL`, `COM1-9`, `LPT1-9`, case-insensitive, before the first dot); otherwise `invalid_argument` from every call (`exists` is false). Spaces inside a name and non-ASCII letters are legal.
- Core now: `ctx.files` uses the storage name rule (any UTF-8 up to 255 bytes, no whitespace), so reserved characters other than the slash and backslash, a leading or trailing dot, device names and 129-byte names are accepted and a space inside a name is refused. Names with a slash, backslash, control character, trailing space or nothing at all are already refused (`29/files_name_refused`, `29/files_name_refused_calls`, green); legal names that already work are `29/files_name_accepted` (green).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`valid_name`, `target`).
- Source: contract only (the probe only wrote `f`, `f2`, `f3`).

### 29/files_case_insensitive
- Contract: names differing only in case address one file; `list()` keeps the case used at creation.
- Core now: names are case-sensitive.
- Touch points: `crates/inkcore/src/ctx/files.rs` (name lookup for the app directory).
- Source: contract only.

### 29/files_list_items, 29/files_list_empty
- Contract: `list()` returns `{ name, size }` items in byte order; an empty folder gives an empty table.
- Core now: bare names (the empty case already matches).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`list` for the app directory). Not pinned: a user adding more than 512 files (`limit_exceeded` from `list`), since an app cannot create them.
- Source: contract only.

### 29/files_read_ranges, 29/files_read_default_len, 29/files_read_arg_errors
- Contract: `read(name, offset, len)` returns at most `len` bytes from `offset` (default 0), the default `len` is 65536, an `offset` at or past the end gives `""`, a negative `offset` or `len` below 1 is `invalid_argument`, a `len` above 65536 is `limit_exceeded`.
- Core now: `offset` and `len` are ignored (a warning is logged) and the whole file is returned.
- Touch points: `crates/inkcore/src/ctx/files.rs` (`read`, files only; `storage:read` keeps whole-file reads).
- Source: probe-confirmed for the default length, `read(name, 65536)` and `read(name, 0, 10)`; the error cases are contract only.

### 29/files_write_limit, 29/files_append_invalid
- Contract: `write` and `append` take at most 262144 bytes per call (`limit_exceeded`, file unchanged); `append` checks the name. (A 262144-byte write is `29/files_write_max`; creating and extending with `append` is `29/files_append`; both green.)
- Core now: no per-call limit; `append` accepts illegal names.
- Touch points: `crates/inkcore/src/ctx/files.rs` (`writer`).
- Source: probe-confirmed for the 262145-byte `write`; `append` is contract only.

### 29/files_count_limit
- Contract: once 512 files exist, creating another through `write` or `append` is `nil, "limit_exceeded"`, while existing files can still change.
- Core now: refused with `no_space`.
- Touch points: `crates/inkcore/src/ctx/files.rs` (`writer`).
- Source: contract only.

### 29/files_no_total_limit
- Contract: the app folder has no total-bytes limit (only a full card gives `io_error`), so appends keep working past 128 MB.
- Core now: the 128 MB storage quota also applies to the app folder (`no_space`).
- Touch points: `crates/inkcore/src/ctx/files.rs` (`writer`: the storage quota applies to both directories today). The check fills 512 full-size files, which takes well under a second.
- Source: contract only.

### 29/files_remove_missing
- Contract: `remove` of an absent app file returns true. (`read` and `size` of an absent file are `not_found` and `exists` is false: `29/files_missing_file`, green.)
- Core now: `remove` returns `nil, "not_found"`.
- Touch points: `crates/inkcore/src/ctx/files.rs` (`remove`).
- Source: contract only (the probe removed existing files only).

Already green and pinned: `29/files_name_refused`, `29/files_name_refused_calls`, `29/files_name_accepted`, `29/files_missing_file`, `29/files_write_max`, `29/files_write_replaces`, `29/files_read_whole`, `29/files_remove_existing`, `29/files_append`, `29/files_path_and_separation` (`ctx.files.path` is `/AppFiles/<app_id>`, probe-confirmed; private storage and app files are separate), `29/files_append_past_call_limit`. Not covered: public folders (`ctx.files:folder`, §29 2.31), `io_error` for a subfolder with a file's name, and the opening of files by the reader.

## Deviations

Deviations (D1, D2) and the note N1 are listed in `spec/deviations.md`. `06/poly_over_128` above is red only because the core does not follow the reader yet.

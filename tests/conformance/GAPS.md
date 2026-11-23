# Core gaps against API 2.33

Every entry below is a rule of the API 2.33 contract that the Rust core does not follow yet. Each one is pinned by a
conformance check listed in `pending.json`; when the core is fixed the check turns green and its pending entry goes.
Source: `probe-confirmed` means the behaviour was measured on the reader (firmware 26112001); `contract only` means the
contract states it and the reader was not measured. File names are relative to the repository root.

## §1 Globals

Already green and pinned: `01/globals_absent` (the globals that must not exist).

## §3 Gray frames

### Gray frames: proposed interface
A frame is gray when one of its `g:image` calls sets `gray = true` (outside `layer:draw`; `ctx.screen.gray_still` is 4, probe-confirmed). `Emu.status()["refresh"]` and `["refreshes"]` report such a frame as `"gray"` (the refresh kinds are described under "Frames" in `README.md`). `Emu.shot()` returns a Pillow image of mode `"L"` for a gray frame, with the gray levels 0, 85, 170 and 255 for 3, 2, 1 and 0 ink steps, and keeps returning mode `"1"` for every other frame. The runner tells the two apart by `image.mode`: a `pixels` entry `[x, y, ink, level]` always checks ink (`1` only for black, the 3-step level) and also checks the level 0-3 when the shot is gray, so the level is ignored on a 1-bit frame. The core stays 1-bit for normal frames; only a gray frame needs the 4-step buffer.
- Touch points: `crates/inkcore/src/draw/canvas.rs` and `crates/inkcore/src/draw/image.rs` (frame buffer and `g:image` options), `crates/inkcore/src/emu.rs` (frame presentation, `frame`), `crates/inkemu-py/src/lib.rs` and `src/inkemu/emu.py` (`_image`, `shot`).

### 03/gray_frame_refresh, 03/gray_next_frame_full
- Contract: `gray = true` on a `g:image` makes the frame a gray one, shown with the full-refresh gray waveform; the next ordinary frame is full again and the one after goes back to partial; `gray = true` inside `layer:draw` is ignored.
- Core now: `gray` is ignored and no refresh kind is recorded.
- Touch points: `crates/inkcore/src/draw/image.rs`, `crates/inkcore/src/ctx/layers.rs`, `crates/inkcore/src/emu.rs`.
- Source: contract only.

### 03/gray_pixels
- Contract: every color of a gray frame is scaled to `gray_still` steps as `(color * gray_still) // 16`, and a 1-bit image keeps only black and white. The check expects colors 3, 4, 8, 12 and 15 to come out as 0, 1, 2, 3 and 3 steps.
- Core now: the shot is always 1-bit, so a rectangle of color 8 or more is full ink and the levels cannot be told apart.
- Touch points: as for the interface above.
- Source: contract only.

Already green and pinned: `03/color_threshold`, `03/device_profile` (the profile values, with the keyboard height as deviation D1), `03/gray_layer_ignored` (green while no frame is gray; it guards the gray frames above).

## §5 Presenting a frame

Already green and pinned: `05/cmd_cap_1025`, `05/cmd_cap_clear`, `05/cmd_cap_text_box_lines`, `05/cmd_cap_text_box_counter` (the rectangles that still fit after a refused `text_box`), `05/cmd_cap_underline`, `05/cmd_cap_underline_text`, `05/invalidate_args`, `05/invalidate_valid`, `05/perf_info`, `05/refresh_first_frame` (an inkemu choice: the contract does not classify the first frame), `05/refresh_plain`, `05/refresh_full_arg`, `05/refresh_full_wins`, `05/refresh_full_in_draw`, `05/refresh_auto_full`, `05/refresh_defer_blocks_auto`, `05/defer_args`.

## §6 Drawing

All §6 checks are green and pinned: `06/g_methods`, `06/canvas_inactive`, `06/polyline_points`, `06/polygon_points`, `06/poly_over_128` (deviation D2), `06/pattern_invalid_args`, `06/arc_full_turn`, `06/circle_fill_pixels`, `06/image_invalid_value`, `06/image_layer_self`, `06/image_src_invalid`, `06/image_released`, and the valid cases `06/g_methods_present`, `06/polyline_points_valid`, `06/polygon_points_valid`, `06/pattern_valid_args`, `06/image_src_valid`, `06/arc_quadrant`, `06/pattern_unknown_kind`, `06/polygon_fill_stroke`, `06/rect_radius_rows`, `06/rect_stroke_inward`, `06/rect_zero_size`, `06/success_returns_nothing`.

## §7 Text and fonts

All §7 checks are green and pinned: `07/text_invalid_utf8`, `07/text_rtl`, `07/text_limits_call`, `07/text_limit_frame`, `07/layout_limit`, `07/measure_bold`, `07/measure_control_chars`, `07/layout_results`, `07/layout_paging`, `07/layout_matches_text_box`, `07/font_object`, `07/fonts_size_range`, `07/fonts_size_sample`, `07/font_metrics`, and the valid cases `07/font_line_heights`, `07/font_object_fields`, `07/fonts_size_valid`, `07/fonts_size_sample_valid`, `07/measure_other_opts`, `07/text_utf8_valid`, `07/text_latin`, `07/text_limit_frame_fit`, `07/text_limits_call_max`, `07/fonts_size_tier`.

## §9 Private files

Already green and pinned: `09/storage_usage`, `09/storage_name_valid`, `09/storage_name_refused`, `09/storage_file_size_max`, `09/storage_remove_existing`, `09/storage_missing_file`, `09/storage_write_replaces`, `09/storage_list_order` (byte order is contract only; the probe sample was alphabetical), `09/storage_full_quota` (512 full-size files are exactly the 128 MB total, so the total quota is not reachable separately and is covered by this check).

## §12 Clock, log and memory

Already green and pinned: `12/log_and_clock`, `12/clock_fields` (date and time fields, epoch and local seconds), `12/clock_snapshot`, `12/log_returns_nothing`, `12/clock_millis`.

## §14 Networking

All network checks use replay fixtures on `.test` hosts, so they pass the same way on any core that answers fixtures. Unverified on 26112001 (no radio in the probe): completion of `PUT`, `PATCH`, `DELETE` and the rest; the checks follow the contract (note N1 in `spec/deviations.md`).

All §14 checks are green and pinned: `14/net_methods_accepted`, `14/net_head_no_body`, `14/net_method_case`, `14/net_body_get_head`, `14/net_body_rules` (`limit_exceeded` by §18's general rule), `14/net_header_invalid`, `14/net_url_length` (`limit_exceeded` by §18's general rule), `14/net_timeout_range`, `14/net_busy`, `14/net_response_fields`, `14/net_response_error_status`, `14/net_body_cap`, `14/net_save_files_2xx`, `14/net_save_storage_2xx`, `14/net_save_non_2xx`, `14/net_save_invalid`, `14/net_save_storage_limit`, `14/net_save_files_limit`, `14/net_save_files_quota`, `14/net_keep_wifi`, and the valid cases `14/net_methods_basic`, `14/net_method_empty`, `14/net_body_allowed`, `14/net_url_scheme`, `14/net_timeout_accepted`, `14/net_keep_wifi_types`, `14/net_body_cap_exact`, `14/net_header_limits_ok`, `14/net_redirect_not_followed`. Not covered: `ctx.net:websocket` (not emulated), `no_network`, `timeout`, `io_error` and `out_of_memory` answers (a fixture cannot express them), unfixtured requests (replay answers `no_fixture`, which the contract does not know), the `content-type` default, header joining of duplicates, the slot a cancelled request holds until the host aborts it, the storage total-bytes quota via save, and `io_error` on a full card.

## §15 Lock screen

Lock-screen frames count as app frames in `status()["refresh"]` and `status()["refreshes"]` (the refresh kinds are described under "Frames" in `README.md`): after `lock` the list has the session-start frame of the app first, then the frames of the first session, then one per timer wake. The checks below assume that. Frame counts of a networked session follow the contract's step list (one frame at once with the stored data when requests or timers remain after `on_load`, one final frame).

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

Already green and pinned: `25/act_refresh`, `25/act_inactive_no_lock` (refused `sleep`/`power_off` calls do not lock the device), `25/act_args` (unknown name is `nil, "invalid_argument"` - probe-confirmed; a non-string name raises). Not covered: `act("power_off")` with its confirmation dialog and `busy` (contract only, needs system dialog support).

## §26 Encoding, hashes, random

Already green and pinned: `26/encoding`, `26/hash_input_max`, `26/encoding_input_max`, `26/base64_decode`, `26/url_encode_decode`, `26/random_bytes_range`, `26/hash_hmac_sha256` (the probe's `k`/`m` vector, RFC 4231 cases 2 and 6), `26/hex_decode`, `26/random_uuid`, `26/hash_crc32` and `26/hash_sha1` (probe-confirmed; fixed in the core).

## §29 App files

Already green and pinned: `29/files_name_refused`, `29/files_name_refused_calls`, `29/files_name_accepted`, `29/files_missing_file`, `29/files_write_max`, `29/files_write_replaces`, `29/files_read_whole`, `29/files_remove_existing`, `29/files_append`, `29/files_path_and_separation` (`ctx.files.path` is `/AppFiles/<app_id>`, probe-confirmed; private storage and app files are separate), `29/files_append_past_call_limit`. Not covered: public folders (`ctx.files:folder`, §29 2.31), `io_error` for a subfolder with a file's name, and the opening of files by the reader.

## Deviations

Deviations (D1, D2) and the note N1 are listed in `spec/deviations.md`. The core follows D2: `06/poly_over_128` is green.

# Core gaps against API 2.33

The Rust core follows every rule listed below, and each one is pinned by a green conformance check; `pending.json` is
empty. A check that turns red is a regression. The "Not covered" notes name what no check pins yet.
Source: `probe-confirmed` means the behaviour was measured on the reader (firmware 26112001); `contract only` means the
contract states it and the reader was not measured. File names are relative to the repository root.

## §1 Globals

Already green and pinned: `01/globals_absent` (the globals that must not exist).

## §3 Gray frames

All §3 checks are green and pinned: `03/color_threshold`, `03/device_profile` (the profile values, with the keyboard height as deviation D1), `03/gray_frame_refresh`, `03/gray_next_frame_full`, `03/gray_pixels` (the gray levels as described under "Gray frames" in `README.md`), `03/gray_layer_ignored`.

## §5 Presenting a frame

Already green and pinned: `05/cmd_cap_1025`, `05/cmd_cap_clear`, `05/cmd_cap_text_box_lines`, `05/cmd_cap_text_box_counter` (the rectangles that still fit after a refused `text_box`), `05/cmd_cap_underline`, `05/cmd_cap_underline_text`, `05/invalidate_args`, `05/invalidate_valid`, `05/perf_info`, `05/refresh_first_frame` (an inkemu choice: the contract does not classify the first frame), `05/refresh_plain`, `05/refresh_full_arg`, `05/refresh_full_wins`, `05/refresh_full_in_draw`, `05/refresh_auto_full`, `05/refresh_defer_blocks_auto`, `05/defer_args`.

## §6 Drawing

All §6 checks are green and pinned: `06/g_methods`, `06/canvas_inactive`, `06/polyline_points`, `06/polygon_points`, `06/poly_over_128` (deviation D2), `06/pattern_invalid_args`, `06/arc_full_turn`, `06/circle_fill_pixels`, `06/image_invalid_value`, `06/image_layer_self`, `06/image_src_invalid`, `06/image_released`, and the valid cases `06/g_methods_present`, `06/polyline_points_valid`, `06/polygon_points_valid`, `06/pattern_valid_args`, `06/image_src_valid`, `06/arc_quadrant`, `06/pattern_unknown_kind`, `06/polygon_fill_stroke`, `06/polygon_pixel_centres` (deviation D16), `06/rect_radius_rows`, `06/rect_stroke_inward`, `06/rect_zero_size`, `06/success_returns_nothing`.

## §7 Text and fonts

All §7 checks are green and pinned: `07/text_invalid_utf8`, `07/text_rtl`, `07/text_limits_call`, `07/text_limit_frame`, `07/layout_limit`, `07/measure_bold`, `07/measure_control_chars`, `07/layout_results`, `07/layout_paging`, `07/layout_matches_text_box`, `07/font_object`, `07/fonts_size_range`, `07/fonts_size_sample`, `07/font_metrics`, and the valid cases `07/font_line_heights`, `07/font_object_fields`, `07/fonts_size_valid`, `07/fonts_size_sample_valid`, `07/measure_other_opts`, `07/text_utf8_valid`, `07/text_latin`, `07/text_limit_frame_fit`, `07/text_limits_call_max`, `07/fonts_size_tier`.

## §9 Private files

Already green and pinned: `09/storage_usage`, `09/storage_name_valid`, `09/storage_name_refused`, `09/storage_file_size_max`, `09/storage_remove_existing`, `09/storage_missing_file`, `09/storage_write_replaces`, `09/storage_list_order` (byte order is contract only; the probe sample was alphabetical), `09/storage_full_quota` (512 full-size files are exactly the 128 MB total, so the total quota is not reachable separately and is covered by this check), `09/storage_name_invalid`, `09/storage_name_exists_invalid`, `09/storage_file_size_limit`, `09/storage_file_count_limit`, `09/storage_remove_missing`.

## §12 Clock, log and memory

Already green and pinned: `12/log_and_clock`, `12/clock_fields` (date and time fields, epoch and local seconds), `12/clock_snapshot`, `12/log_returns_nothing`, `12/clock_millis`, `12/log_methods`, `12/clock_revision_start`, `12/clock_weekday_sunday`, `12/clock_weekday_saturday`.

## §14 Networking

All network checks use replay fixtures on `.test` hosts, so they pass the same way on any core that answers fixtures. Unverified on 26112001 (no radio in the probe): completion of `PUT`, `PATCH`, `DELETE` and the rest; the checks follow the contract (note N1 in `spec/deviations.md`).

All §14 checks are green and pinned: `14/net_methods_accepted`, `14/net_head_no_body`, `14/net_method_case`, `14/net_body_get_head`, `14/net_body_rules` (`limit_exceeded` by §18's general rule), `14/net_header_invalid`, `14/net_url_length` (`limit_exceeded` by §18's general rule), `14/net_timeout_range`, `14/net_busy`, `14/net_response_fields`, `14/net_response_error_status`, `14/net_body_cap`, `14/net_save_files_2xx`, `14/net_save_storage_2xx`, `14/net_save_non_2xx`, `14/net_save_invalid`, `14/net_save_storage_limit`, `14/net_save_files_limit`, `14/net_save_files_quota`, `14/net_keep_wifi`, and the valid cases `14/net_methods_basic`, `14/net_method_empty`, `14/net_body_allowed`, `14/net_url_scheme`, `14/net_timeout_accepted`, `14/net_keep_wifi_types`, `14/net_body_cap_exact`, `14/net_header_limits_ok`, `14/net_redirect_not_followed`. Not covered: `ctx.net:websocket` (not emulated), `no_network`, `timeout`, `io_error` and `out_of_memory` answers (a fixture cannot express them), unfixtured requests (replay answers `no_fixture`, which the contract does not know), the `content-type` default, header joining of duplicates, the slot a cancelled request holds until the host aborts it, the storage total-bytes quota via save, and `io_error` on a full card.

## §15 Lock screen

Lock-screen frames count as app frames in `status()["refresh"]` and `status()["refreshes"]` (the refresh kinds are described under "Frames" in `README.md`): after `lock` the list has the session-start frame of the app first, then the frames of the first session, then one per timer wake. Frame counts of a networked session follow the contract's step list (one frame at once with the stored data when requests or timers remain after `on_load`, one final frame).

All §15 checks are green and pinned: `15/lock_reason_first`, `15/lock_reason_timer`, `15/lock_elapsed_first`, `15/lock_timer_alignment`, `15/lock_net_first`, `15/lock_net_undeclared`, `15/lock_net_interval_default`, `15/lock_net_interval_elapsed`, `15/lock_net_interval_blocks`, `15/lock_net_interval_clamp`, `15/lock_net_interval_clamp_low`, `15/lock_prep_first`, `15/lock_first_frames`, `15/lock_prep_timer`, `15/lock_prep_cap`, `15/lock_done_early`, `15/lock_invalidate_full_prep`, `15/lock_invalidate_plain_prep`, `15/lock_refresh_kinds`, `15/lock_gray_then_normal`, `15/lock_after_noop`, `15/lock_budget_inactive`, `15/lock_set_interval_args` (the `invalid_argument` code comes from the §0 out-of-range rule; §15 gives only the range), `15/lock_set_interval_effect`. The prelude records reach `emu.log` from a lock session (these checks read them). Not covered: the system lock screen on a missing entry, error or timeout in the script (default picture, no more wakes), low battery, the three failed Wi-Fi connections, `ctx.lockscreen` wallpaper calls and the confirmation dialog.

## §25 System actions

Already green and pinned: `25/act_refresh`, `25/act_inactive_no_lock` (refused `sleep`/`power_off` calls do not lock the device), `25/act_args` (unknown name is `nil, "invalid_argument"` - probe-confirmed; a non-string name raises), `25/act_inactive`, `25/act_sleep_key`, `25/act_sleep_tap`. Not covered: `act("power_off")` with its confirmation dialog and `busy` (contract only, needs system dialog support).

## §26 Encoding, hashes, random

Already green and pinned: `26/encoding`, `26/hash_input_max`, `26/encoding_input_max`, `26/base64_decode`, `26/url_encode_decode`, `26/random_bytes_range`, `26/hash_hmac_sha256` (the probe's `k`/`m` vector, RFC 4231 cases 2 and 6), `26/hex_decode`, `26/random_uuid`, `26/hash_crc32` and `26/hash_sha1` (probe-confirmed; fixed in the core), `26/hash_input_limit`, `26/encoding_input_limit`, `26/base64_encode_std`, `26/base64_url_alphabet`, `26/base64_decode_url_unpadded`, `26/url_decode_invalid`, `26/url_query`, `26/random_bytes`.

## §29 App files

Already green and pinned: `29/files_name_refused`, `29/files_name_refused_calls`, `29/files_name_accepted`, `29/files_missing_file`, `29/files_write_max`, `29/files_write_replaces`, `29/files_read_whole`, `29/files_remove_existing`, `29/files_append`, `29/files_path_and_separation` (`ctx.files.path` is `/AppFiles/<app_id>`, probe-confirmed; private storage and app files are separate), `29/files_append_past_call_limit`, `29/files_name_illegal`, `29/files_name_illegal_calls`, `29/files_name_inner_space`, `29/files_case_insensitive`, `29/files_list_items`, `29/files_list_empty`, `29/files_read_ranges`, `29/files_read_default_len`, `29/files_read_arg_errors`, `29/files_write_limit`, `29/files_append_invalid`, `29/files_remove_missing`, `29/files_count_limit`, `29/files_no_total_limit`. Not covered: public folders (`ctx.files:folder`, §29 2.31), `io_error` for a subfolder with a file's name, the opening of files by the reader, and `list()` returning `nil, "limit_exceeded"` when more than 512 files were put in `appfiles/` on disk (inkemu lists them all).

## Deviations

Deviations (D1–D16) and the note N1 are listed in `spec/deviations.md`, each with its pinning checks.

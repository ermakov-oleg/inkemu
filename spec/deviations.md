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
- inkemu does: follows the reader for 129 or more points (`nil, "limit_exceeded"`).
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

### D4 - a repeat key in ui:handle_event

- Contract says: keys with phase `repeat` are events since 2.10 (§4); `handle_event` lists results only for `down`
  and `up`, and an event with no matching action gives `nil, nil` (§16).
- Reader does: a key event with phase `repeat` returns `nil, "invalid_argument"`, at every package minor (the
  reader's UI library run as a black box with our scenarios, and the probe).
- inkemu does: `nil, "invalid_argument"`. Whether such a key turns focus drawing on was not measured; inkemu leaves
  it as it was.
- Status: measured on 26112001.
- Checks: `16/events_keys`.

### D5 - ok up needs its ok down

- Contract says: the `up` of `ok` activates the focused control when focus did not move (§16).
- Reader does: it activates only after an `ok` down with focus unchanged since; an `ok` up with no `ok` down before
  it, or a second `ok` up after one down, returns `nil, nil`. A frame drawn in between does not break the pair.
- inkemu does: the same.
- Status: measured on 26112001.
- Checks: `16/events_keys`.

### D6 - pointer hits at the press point

- Contract says: a `tap`, `double_tap` or `hold` acts on the control hit (§16); it does not name the point, and an
  event's `x, y` is where that event happened (§4).
- Reader does: hits at `down_x, down_y`, and at `x, y` only when the event has no press point.
- inkemu does: the same.
- Status: measured on 26112001.
- Checks: `16/events_pointer`.

### D7 - the ends of order navigation

- Contract says: before 2.13 an arrow moves focus to the previous or next control (§16); it does not say what
  happens at the first or last control.
- Reader does: returns the same control's id with `focus` there, rather than `nil, nil`.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library and the probe).
- Checks: `16/nav_order_before_2_13`.

### D8 - a pointer event on a device with no touch input

- Contract says: with no touch input, focus is drawn at all times (§16).
- Reader does: a pointer event passed to `handle_event` turns focus drawing off there too, until the next key
  event. Such a device produces no pointer events of its own, so only an app that builds one sees this.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/focus_drawn_only_after_key` (`keyless.after_pointer`).

### D9 - a container that runs into the 128 controls of a frame

- Contract says: the call that goes past 128 controls returns `nil, "limit_exceeded"` and its control is not drawn
  (§16).
- Reader does: a `row` or `column` whose children go past the limit returns `limit_exceeded`, but the container and
  the children registered before the limit stay: they are drawn, hit and take focus.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/layout_limits` (`container_at_limit.k2`).

### D10 - nesting levels in a frame

- Contract says: more than 16 nested levels of containers gives `nil, "limit_exceeded"`, in a frame and in
  `ui:layout` alike (§16).
- Reader does: in a frame the control inside the innermost container counts as a level, so 16 containers with a
  control in the last one already give `limit_exceeded` from the outermost call and nothing is drawn; a spacer there
  does not count. `ui:layout` takes 16 containers with a control inside and refuses 17.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/layout_limits` (`frame_16_levels_and_a_control`).

### D11 - row and column before 2.15 are not clipped

- Contract says: for packages before 2.15, a `row` or `column` clips its children to its box (§16).
- Reader does: no clipping; a child larger than its container is drawn whole.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/row_column_before_2_15` (`not_clipped`).

### D12 - fit widths of switch and tabs from 2.16

- Contract says: the content width of a `switch` uses its text width, that of `tabs` its widest item (§16).
- Reader does: from 2.16 both are measured bold, as the 2.16 defaults draw them: the switch text unless `bold =
  false`, and every tab item unless `bold = false`, even with no selected cell.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/layout_content_sizes` (`switch_bold`, `tabs_bold`).

### D13 - reserve without w = "fit"

- Contract says: from 2.16 `reserve` sets the width of a `label` or `button` only when its `w` is `"fit"`, and does
  nothing otherwise (§16).
- Reader does: the reserved text widens the content width wherever the width comes from the content, such as a
  child with no `w` left at its content width by `align_items = "start"`, or a row child with no `w` and no `grow`.
- inkemu does: the same.
- Status: measured on 26112001 (the reader's UI library run as a black box).
- Checks: `16/layout_content_sizes` (`reserve_without_fit`).

### D14 - container children checked one by one in a frame

- Contract says: a control whose values break the rules returns `nil, "invalid_argument"` and is not drawn (§16).
- Reader does: a `row` or `column` in a frame checks and registers its children in declaration order. When a child is
  refused (a bad field of its own, or a box placed past 32767, see D15), the call returns `invalid_argument` there,
  and the container and the children registered before stay: they are drawn, hit and take focus. When the 128 controls
  run out first, the call returns `limit_exceeded` (D9), even if a later child is bad.
- inkemu does: the same, for a control's own fields and its placed box. Sizing fields are checked for the whole tree
  before anything is registered.
- Status: measured on 26112001 (the reader's UI library run as a black box) for a bad field on the first child,
  a bad field on the last child after the limit, and children placed past 32767; a bad field of its own on a child
  in the middle, after valid ones, was not measured and is inferred from those cases.
- Checks: `16/layout_limits` (`bad_child.before`, `child_past_32767.before`).

### D15 - coordinates up to 32767

- Contract says: `x`, `y` are integers and `w`, `h` integers of at least 1, with no upper bound; a `ui:layout` rect
  the same (§16).
- Reader does: a `ui:layout` rect wider or taller than 32767 is `invalid_argument`; a container child placed at an
  `x` past 32767 is refused when it is registered (D14).
- inkemu does: the same; a control's `x` and `y` must lie in -32768..32767 and its `w` and `h` be at most 32767, at
  top level as in a container; an outermost container's and a `ui:layout` rect's `w` and `h` at most 32767, and
  their `x` and `y` in -32768..32767.
- Status: measured on 26112001 for the rect's width and a child's `x` (the reader's UI library run as a black box);
  the other bounds, the rect's and the outermost container's `x` and `y` among them, follow from 16-bit
  coordinates and were not measured.
- Checks: `16/layout_errors` (`rect_w_32768`, `rect_h_32768`, `rect_x_32768`, `rect_y_minus_32769`),
  `16/layout_limits` (`child_past_32767`), `16/id_rules` (`row_x_32768`, `row_y_minus_32769`),
  `16/row_column_before_2_15` (`x_32768`).

### D16 - polygon and thick-line pixels

- Contract says: a filled `polygon` covers the pixels whose centre is inside by the even-odd rule, and a `line`
  thicker than 1 the pixels closer than `line_width / 2` to the segment (§6). It does not say where a point lies
  within its pixel or how a centre on the boundary counts.
- Reader does: points of `line`, `polyline` and `polygon` are the centres of the named pixels: point `(x, y)` is
  `(x + 0.5, y + 0.5)`, as for the `circle` centre. A filled `polygon` covers the pixels whose centre is inside by
  the even-odd rule and draws no outline; a centre exactly on the boundary counts as inside when the interior lies
  immediately to its right, or, where the boundary is horizontal there, immediately below it. A `line` thicker than
  1 covers the pixels whose centre is strictly closer than `line_width / 2` to the segment between its end pixels'
  centres (round ends).
- inkemu does: the same. Mostly this settles what §6 leaves open; the one departure from a literal reading is that a
  filled polygon has no outline. An even `line_width` along an axis follows the rule literally, so a horizontal line
  of width 2 is one pixel tall.
- Status: measured on 26112001 for filled polygons and for widths 2 and 3 on slanted segments (the "Today" app's
  screens in x4pro-emu, 0 px apart); even widths along an axis were not measured.
- Checks: `06/polygon_pixel_centres`, `06/polygon_fill_stroke`.

## Notes

### N1 - completion of PUT, PATCH and DELETE

- Contract says: the 13 listed methods are accepted and their completion reaches the callback (§14).
- Reader does: the three methods are accepted synchronously (a job is returned). Whether the callback completes was not
  measured: the probe has no radio, and an older firmware (26102401) hung on them.
- inkemu does: follows the contract; every listed method completes through the callback.
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
- `ctx.ui` arrows follow the package manifest's `api`, not the firmware's: a 2.12 package moves in declaration order
  (skipping disabled controls, ignoring `nav`), a 2.13 one by position. Checks: `16/nav_order_before_2_13`,
  `16/nav_spatial`.
- `ctx.ui` 2.15 layout: the §16 `ui:layout` example, the rounding of grow and shrink, the `justify_content` weights,
  `overflow`, the halftone bands and rounded rows of a container's fill, and where a divider goes (1 px into
  the earlier child when the gap is 0) come out as the contract gives them. A package for 2.15 gets the container
  look in plain colours; halftone starts at 2.16. Checks: `16/layout_*`, `16/container_appearance*`,
  `16/overflow_field`.

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
- `ctx.ui`, measured the same way: `handle_event` gives `nil, nil` for a key without `key`, `back`, unknown keys, arrow
  ups, pointer `down`/`move`, other gestures and pointer events without a point, and `nil, "invalid_argument"` for a
  key without `phase`; any key event turns focus drawing on and any pointer event turns it off, even when the result
  is `nil, nil`; `hold` leaves focus where it was; `ui:focus(id, 2.0)` keeps part 2. Checks: `16/events_keys`,
  `16/events_pointer`, `16/focus_drawn_only_after_key`, `16/focus_api`.
- `ctx.ui`, not measured: `begin_frame` takes as a canvas any table with the canvas's `rect` and `text_box` methods,
  since Lua cannot tell host objects apart. A control's box holds the points `x .. x + w - 1` for hits, and boxes that
  only touch do not overlap for the arrows. A key whose `phase` is neither `down` nor `up` (any other value, not only
  `repeat` or none) is `invalid_argument`, and a refused event leaves focus drawing as it was. A pointer event is
  accepted whatever its `phase` (none or unknown gives `nil, nil` and turns focus drawing off). `ok`
  on a kept id that is disabled in the last frame gives `nil, nil`. An arrow from a kept id that `ui:focus` set to a
  disabled control starts from that control's place.
- `ctx.ui` containers, as measured with the reader's UI library run as a black box: before 2.15 a `row` or `column`
  needs an integer `w` and `h` on every child and places them one after another from its corner with `gap`, ignores
  the 2.15 fields, and refuses `spacer` children; a hidden child's `id` is not checked against the keys of the
  `ui:layout` result; a percentage in a fitted container resolves against the size found for it. Checks:
  `16/row_column_before_2_15`, `16/layout_content_sizes`, `16/layout_hidden_and_false`.
- `ctx.ui` containers, measured later (oracle scenario s17) and followed by inkemu:
  - In a frame a control child without `id` or with `id = ""`, or a child without `kind`, makes the whole call
    `invalid_argument`; `ui:layout` takes both and refuses only `id = ""`.
  - `ui:layout` counts all the child tables of the tree against 1024 before it checks any field, and does not check
    control fields that do not size the child, such as `enabled`.
  - A `label`, `button` or `switch` child takes a `text` that is not a string and hands it to `g:text_box` as it is,
    except where the text must be measured: a `button` or `switch` with `w = "fit"`, or a `label` without a numeric
    `h`, is `invalid_argument`.
  - See D13 (`reserve`), D14 (children checked one by one) and D15 (32767).
  - The grow and shrink splits multiply in 32-bit integers and wrap with huge weights (six children of `w = 400`
    and `shrink = 1000` in a 480 px row give one child 1869 px). inkemu gives the same sizes because it runs the
    same cumulative formula in 32-bit integers; the s17 oracle steps in `tests/test_inkui_oracle.py` pin those
    cases.
- `ctx.ui` containers, not measured:
  - A frame checks the structure and the sizing fields of the whole tree first (kinds, ids, sizes, the fields a
    content size reads), so a bad one anywhere refuses the call before anything is registered; only a control's other
    fields and its placed box are checked one by one (D14). A missing `id` or `kind` was measured only on the first
    child.
  - A part inkemu does not implement yet raises only when nothing in that first pass is invalid.
  - The 1024 children are counted in frames too.
  - A control declared at top level is held to the same 32767 limits as a child (D15).
  - A whole float that a 32-bit Lua cannot hold as an integer (such as `3e9` or `1e10`) is not an integer: in any
    integer field of a control, a container, a child or a `ui:layout` rect it gives `nil, "invalid_argument"`.
  - A text that is not a string counts as 0 px wide where an unset `w` takes the content width.
  - A hidden child's `id` is not reserved in the frame; a zero-size child is not registered, so its `id` is free too.
  - `divider = false` is the same as no divider, and `children = nil` as no children.
  - Dividers are drawn between neighbours that are not spacers even when one of them has zero size.
  - Before 2.15, a nested container places its own children from its own corner, and `gap` must be a non-negative
    integer.
  - A rounded halftone block whose radius takes its whole height gets no middle row.
  - The `gap` between `tabs` or `radio` cells is any integer from 0 (the contract gives no upper bound), and their
    1 to 64 `items` are checked the same way in a frame and in `ui:layout`.
- `ctx.ui` radio, as measured with the reader's UI library run as a black box: a radio works at every API minor (as
  the other 2.13 and 2.14 controls do), a `nav` on it is accepted and ignored (the contract says composites take no
  `nav`), a 3 px cell draws no dot, and the fallback to the first control lands on item 1 while `ui:focus(id)` without
  an item, or with one the radio lacks, lands on the value item. Checks: `16/radio_cells`, `16/radio_focus_parts`,
  `16/radio_change_activate`, `16/limits`, `16/nav_override`.
- `ctx.ui` radio, not measured:
  - A cell gets the value dot only when it is at least 11 px tall, so that the dot's diameter is at least 1.
  - With `font_min`, each item's font shrinks until its text fits the width after the ring.
  - A tap in the gap between two cells hits no part of the radio and goes to what lies below.
  - An item that is `""` still gets its `text_box`.
  - Until `end_frame` settles it, a focus that `ui:focus` set on a composite without an item, or with an item it
    lacks, acts as its default part for the arrows and for `ok`; `ok` on a radio that is disabled in the last frame
    gives `nil, nil`, as on a disabled plain control.
  - A `nav` on a radio is checked as on the other controls (a non-table, or a value that is not a string, is
    `invalid_argument`), then ignored.
  - A cell `gap` wider than the box counts as the box, a cell never gets a negative size, and the text width after
    the ring is at least 0.
  - A `nav` entry naming a composite whose default part is unavailable falls back to the search by position.
  - With a skin that has a disabled look, a disabled radio draws each cell in it and gets no dots.
- `ctx.ui` switch, as measured with the reader's UI library run as a black box: `selected` is not type-checked (any
  value but nil and false is on), a `badge` is ignored, a `radius` only shapes the focus frame (radius + 3 when
  positive), `focus_style` does not change a switch's look, and `bold = false` leaves `bold` out of the text call.
  Checks: `16/switch_geometry`.
- `ctx.ui` switch, not measured: the text is left out when the room before the capsule is 0 or less, and with a note
  of odd height the note box takes the extra pixel. With a skin that has a focused look, focus draws it at the
  capsule as well as the switch's own frame; with a disabled look, a disabled switch gets no dots.
- `ctx.ui` images, as measured with the reader's UI library run as a black box: an image button draws its image
  scaled to the box (`invert` only when selected, never for emphasis or focus), then the border rect without a `fill`
  key, the focus frame, the text with no colour of its own, the badge and the dots; focus never fills it. A top-level
  `ui:image` passes any non-nil `source` to the canvas as it is and is neither hit nor focusable; a button `image` and
  a layout child's `source` must be an image object (a table with only `width` and `height` is not one). Checks:
  `16/image_button`.
- `ctx.ui` skins, as measured with the reader's UI library run as a black box (oracle scenario s24) and followed by
  inkemu:
  - `create` holds only `edge` to the image (left + right below its width, top + bottom below its height, a variant
    with its own image and edge); `pad` is any 0..255 in the same CSS forms, even wider than the image. A whole
    float such as `text_color = 2.0` is taken, and a variant name it does not know (`hover`) is ignored whatever it
    holds.
  - `ui:skin` does not type-check the fields of `st`: only `true` sets `focused`, `selected` or `emphasis`
    (`focused = 1` draws the unfocused look), and only `enabled = false` clears `enabled`. The contract calls them
    booleans.
  - The text area after `pad` is at least 1 px wide and tall, also when the box is smaller than the pad; a
    variant's own `edge` does not move it.
  - A skin's disabled look replaces the white dots; without one the usual look is covered. A selected look with
    focus gets the ring even when the skin has a focused look; only the focused look itself goes without. The ring
    is the square 2 px frame 3 px out, drawn after the text, and the outline focus style draws a skin without a
    focused look the same way. A skinned button's `radius` and `border` change nothing.
  - Inside a frame, `ui:skin` draws on the canvas at once, below the controls that `end_frame` paints.
  - Measured later (oracle scenario s27): a skin without a selected look draws its normal look with `invert = true`
    and the text in 15 minus the look's text colour, on a button and on a radio cell alike; without a disabled look
    a disabled button, switch or radio gets its usual look and the dots over the whole control. A skinned switch
    with focus gets only its own frame around the whole control, after its text; a skinned radio cell with focus
    gets the square frame 3 px out after its text. A container's skin replaces its background and border, and its
    divider is still drawn, after the skin and before the children.
  Checks: `16/skins_validation`, `16/skin_nine_slice`.
- `ctx.ui` skins, not measured:
  - `ui:skin` with `focused` and `enabled = false` on a skin with a disabled look draws that look with the ring.
  - An emphasis look with focus gets the ring like a selected one, also when the skin has a focused look; the ring
    stays square when the button sets a `radius`; the outline focus style uses a skin's focused look as the fill
    style does.
  - A skin whose image was released draws nothing of it (the canvas refuses those `g:image` calls) and still gets
    its text (check `16/skin_nine_slice`).
  - A `skins` key that is not a non-empty string, or a skin or variant that is not a table, is `invalid_argument`.

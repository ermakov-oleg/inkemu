# ctx.ui oracle data

Reference results for `tests/test_inkui_oracle.py`.

- `s*.api<minor>.json`: one scenario at one package API minor. `input.steps` are steps we wrote (create a UI object,
  declare frames of controls, pass events, call `ui:focus`, `ui:layout`). `output` holds what the reader's own UI
  library (firmware 26112001) returned and drew for those steps, recorded by running that library unchanged as a
  black box on a recording host with a fixed-pitch font (line height 20/24/30, ascent 17/21/26, 10/12/15 px per
  character; `../fonts-fixed/` is that font as BDF). The files hold inputs and observed outputs only: no firmware
  code. Traces of how the library looked up canvas methods and called the font were removed when copying, since the
  tests do not use them.
- `runner.lua`: our replay of the steps in inkemu, copied into the test app as `index.lua`.

The data cannot be regenerated from this repository: the recording host and the library run outside it. New
scenarios come from the same private harness and are copied here as they are.

Markers in `input.steps`, turned into values by the recorder and by `runner.lua` alike:

- `"@font:<size>"` a system font, `"@g"` the frame's canvas, `"@ctx"` the app's ctx, `"@nil"` an absent value,
  `"@nan"` and `"@inf"` those numbers.
- `"@img:<W>x<H>"` an image object of that size (the test writes a PNG of that size into the package); in results
  it reads back as the same marker.
- `"@rec"`, `"@rect"`, `"@throw"` region and list `draw` callbacks that record `@draw` (and draw a rect, or raise);
  `"@cb"` a callback that records `@cb` with its key (`on_change`, `on_end`) and arguments.

The docked input is a stub on both sides: `ctx.input:edit` records `@input_edit` with its options and returns an edit
object (or `nil, <mode>` after an `input_mode` step), whose `close` records `@ed_close`. The steps `edit_type` and
`edit_end` play the host: they call the live edit's `on_change(ctx, text)` or end it and call `on_end(ctx, text, how)`.

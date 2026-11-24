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

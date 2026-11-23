# Page ⇄ worker messages (`postMessage`)

The worker (`worker.js`) owns the Wasm core; the page (`app.js`) never calls it directly. Messages run in order.

## Page → worker

| Message | Meaning |
|---|---|
| `{type:'open', files, fixtures, fonts, config}` | A fresh core: `files`/`fixtures` are `Map<path, Uint8Array>`, `fonts` is `[small, medium, large]` BDF bytes or null, `config` is the core's create config (`start_local_ms`, `utc_offset_sec`, `net`, `locale`, `app_label`, `random_seed`). Net mode and fonts change only through a new `open`. |
| `{type:'cmd', cmd, ifPending?}` | One core command (`pointer`, `double_tap`, `key`, `type_text`, `submit_text` (the text field: replaces an edit's text, then confirms), `answer`, `wait`, `lock`, `unlock`, `reload`, `restart`, `set_paused`, ...). Numbers must be integers. Before `pointer` and `double_tap` the worker lets the real time since the last tick pass (as the window's `on_release`). `ifPending: [kinds]`: run only if the core has a dialog of one of these kinds open right now, else drop it silently (Y/N and the text field, whose state may lag). |
| `{type:'reload', files, fixtures?}` | Reload with re-read files ("Re-read files" for a folder). |
| `{type:'tick', on}` | Real time on/off; the page turns it off while the tab is hidden. While on and not paused, the worker runs `advance` with the elapsed real time (at most 5 s) every 100 ms. |

## Worker → page

| Message | Meaning |
|---|---|
| `{type:'ready'}` | The Wasm module is loaded, also after a retry that followed a failed first load (the page shows "Loading core…" until then). |
| `{type:'frame', w, h, bits, seq}` | The screen with its plates (core `frame(true)`): rows of `ceil(w/8)` bytes, MSB first, set bit = white. Sent only when `frame_seq` moved. |
| `{type:'state', status, texts, errors, log}` | `status` as the core's state, `errors` all of them, `log` only the new lines (the core's, then the worker's own, such as the CORS line). Sent when the status (clock to the second), the error count, the frame or the log changed. |
| `{type:'error', kind, message}` | Plain words for the log. `kind`: `start` (the app could not start), `core` (module missing or crashed), `fonts` (bad fonts, a start failure), `load` (a `reload` the core rejected), `other` (a failed command). The page clears "Starting…" for all but `other`. |

## After every command, tick and delivery

The worker drains `take_outgoing`, performs each request with `net.js` (`fetch`, `timeout_ms`, default 15 s) and
delivers the answer through the same path; answers meant for an older core are dropped.

Real time keeps running while a dialog is open (the Tk window froze it); `tick` does not look at dialogs.

## A crashed core

A Rust panic aborts the whole module (`WebAssembly.RuntimeError`). The worker reports it, drops the module and
starts the last opened app on a new module at the next `open`, `reload`, or `cmd` `reload`/`restart`.

## Beyond the plan's protocol

`ready`; `ifPending` on `cmd`; `fixtures` on `reload`; the worker's own lines in `state.log`. `net.js` `perform`
returns a `log` line next to the `deliver` arguments; the worker strips it and fills in `<app_id>`.

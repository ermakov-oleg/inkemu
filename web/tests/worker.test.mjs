// The worker's session logic against the real core (build it first with scripts/build-web.sh).
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';

import { loadCore } from '../js/core.js';
import { createSession } from '../js/worker.js';

const ROOT = new URL('../../', import.meta.url);
const MODULE = new URL('../core/inkcore_wasm.js', import.meta.url).href;
const enc = new TextEncoder();

const config = (over = {}) => ({
  start_local_ms: 1_795_165_200_000,
  utc_offset_sec: 0,
  net: 'fixtures+live',
  locale: 'en',
  app_label: 'testapp',
  random_seed: 1,
  ...over,
});

const files = (obj) => new Map(Object.entries(obj).map(([k, v]) => [k, enc.encode(v)]));

async function helloFiles() {
  const out = new Map();
  for (const name of ['manifest.json', 'index.lua', 'lockscreen.lua']) {
    out.set(name, new Uint8Array(await readFile(new URL(`examples/hello/${name}`, ROOT))));
  }
  return out;
}

function harness({ fetchFn, load = () => loadCore(MODULE) } = {}) {
  const msgs = [];
  let t = 0;
  const s = createSession({ post: (m) => msgs.push(m), load, fetchFn, now: () => t });
  return {
    s,
    msgs,
    advanceClock: (ms) => (t += ms),
    states: () => msgs.filter((m) => m.type === 'state'),
    lastState: () => msgs.filter((m) => m.type === 'state').at(-1),
    log: () => msgs.filter((m) => m.type === 'state').flatMap((m) => m.log),
    errors: () => msgs.filter((m) => m.type === 'error').map((m) => m.message),
  };
}

const texts = (st) => st.texts.map((x) => x.text);

test('hello: open, a pointer tap, real time and +1 min', async () => {
  const h = harness();
  await h.s.init();
  assert.equal(h.msgs[0].type, 'ready');
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  const frame = h.msgs.find((m) => m.type === 'frame');
  assert.equal(frame.w, 480);
  assert.equal(frame.h, 800);
  assert.equal(frame.bits.length, 48000);
  assert.ok(texts(h.lastState()).includes('Taps: 0'));
  assert.equal(h.lastState().status.app_id, 'hello');

  const tapMe = h.lastState().texts.filter((x) => x.text === 'Tap me').at(-1);
  const [x, y] = [tapMe.x + (tapMe.w >> 1), tapMe.y + (tapMe.h >> 1)];
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'pointer', phase: 'down', x, y } });
  h.advanceClock(80.6);
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'pointer', phase: 'up', x, y } });
  assert.ok(texts(h.lastState()).includes('Taps: 1'));

  const before = h.lastState().status.time;
  h.advanceClock(1500);
  h.s.tick();
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'wait', ms: 60000 } });
  const after = h.lastState().status.time;
  assert.ok(after > before);
  assert.equal(h.errors().length, 0);

  // pause: real time no longer reaches the clock
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'set_paused', paused: true } });
  const n = h.states().length;
  h.advanceClock(3000);
  h.s.tick();
  assert.equal(h.states().length, n);
});

test('a state goes out only when something changed', async () => {
  const h = harness();
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  const n = h.states().length;
  h.advanceClock(100);
  h.s.tick();
  assert.equal(h.states().length, n, 'a 100 ms tick inside the same second is not news');
  h.advanceClock(1000);
  h.s.tick();
  assert.equal(h.states().length, n + 1);
});

test('an endless loop ends in the budget error and the session keeps working', async () => {
  const h = harness();
  const app = files({
    'manifest.json': '{"app_id": "loop", "entry": "index.lua"}',
    'index.lua': 'function on_load(c) while true do end end\nfunction on_draw(c, g) g:clear(0) end\n',
  });
  await h.s.handle({ type: 'open', files: app, fixtures: new Map(), fonts: null, config: config() });
  assert.match(h.lastState().errors[0], /instruction budget/);
  assert.ok(h.log().some((l) => / ERROR .*instruction budget/.test(l)));
  // ticks on a stopped app are quiet
  const before = h.errors().length;
  h.advanceClock(2000);
  h.s.tick();
  assert.equal(h.errors().length, before);
  await h.s.handle({ type: 'reload', files: await helloFiles() });
  assert.ok(texts(h.lastState()).includes('Taps: 0'));
});

test('a request the browser cannot make: no_network reaches the app and the CORS line the log', async () => {
  const fetchFn = async () => {
    throw new TypeError('Failed to fetch');
  };
  const h = harness({ fetchFn });
  const app = files({
    'manifest.json': '{"app_id": "netapp", "entry": "index.lua", "api": "2.2", "permissions": ["net"]}',
    'index.lua':
      "msg = 'none'\n" +
      'function on_draw(c, g) g:clear(0); g:text(0, 0, msg) end\n' +
      "function on_load(c) c.net:request({url = 'https://api.example.com/x'}, function(c, resp)\n" +
      "  msg = 'err:' .. tostring(resp.err); c:invalidate() end) end\n",
  });
  await h.s.handle({ type: 'open', files: app, fixtures: new Map(), fonts: null, config: config() });
  await new Promise((r) => setTimeout(r, 20));
  h.advanceClock(1000);
  h.s.tick();
  assert.ok(texts(h.lastState()).includes('err:no_network'), JSON.stringify(texts(h.lastState())));
  const line = h.log().find((l) => l.includes('failed in the browser'));
  assert.match(line, /^\d\d:\d\d:\d\d\.\d{3} net: GET https:\/\/api\.example\.com\/x failed in the browser/);
  assert.ok(line.endsWith('tests/fixtures/netapp/net/'));
});

test('a crashed module is dropped and Reload starts the last app on a new one', async () => {
  let loads = 0;
  const real = await loadCore(MODULE);
  const load = async () => {
    loads++;
    if (loads > 1) return real;
    return {
      create: (opts) => {
        const c = real.create(opts);
        return {
          cmd: (o) => {
            if (o.cmd === 'key') throw new WebAssembly.RuntimeError('unreachable');
            return c.cmd(o);
          },
          frame: (d) => c.frame(d),
          destroy: () => c.destroy(),
        };
      },
    };
  };
  const h = harness({ load });
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'key', name: 'ok' } });
  assert.match(h.errors().at(-1), /emulator core crashed \(unreachable\).*Reload/);
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'key', name: 'ok' } });
  assert.match(h.errors().at(-1), /stopped; press Reload/);
  const n = h.states().length;
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'reload' } });
  assert.equal(loads, 2);
  assert.ok(h.states().length > n);
  assert.ok(texts(h.lastState()).includes('Taps: 0'));
});

test('a failed command is reported, the session goes on', async () => {
  const h = harness();
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'type_text', text: 'x' } });
  assert.match(h.errors().at(-1), /^type_text: no text input is waiting/);
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: [enc.encode('x')], config: config() });
  assert.match(h.errors().at(-1), /^Could not start the app: fonts:/);
});

test('ifPending: an answer runs only while a dialog of that kind is open, otherwise it is dropped silently', async () => {
  const h = harness();
  const app = files({
    'manifest.json': '{"app_id": "dlg", "entry": "index.lua"}',
    'index.lua':
      "msg = 'none'\n" +
      'function on_input(c, ev)\n' +
      "  if ev.gesture == 'tap' then c.dialog:confirm({title = 'Sure?'}, function(c, ok) msg = 'ok ' .. tostring(ok); c:invalidate() end) end\n" +
      "  if ev.gesture == 'double_tap' then msg = 'double'; c:invalidate() end\n" +
      '  return true\n' +
      'end\n' +
      'function on_draw(c, g) g:clear(0); g:text(0, 0, msg) end\n',
  });
  await h.s.handle({ type: 'open', files: app, fixtures: new Map(), fonts: null, config: config() });
  const n = h.msgs.length;
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'answer', ok: true }, ifPending: ['confirm', 'leave'] });
  assert.equal(h.msgs.length, n, 'no dialog: nothing runs, nothing is reported');

  // the tap opens the confirm; Y comes before the page has seen that state
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'tap', x: 10, y: 10 } });
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'type_text', text: 'x' }, ifPending: ['text', 'edit'] });
  assert.equal(h.errors().length, 0, 'type_text for a confirm is dropped, not an error');
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'answer', ok: true }, ifPending: ['confirm', 'leave'] });
  assert.ok(texts(h.lastState()).includes('ok true'));
  assert.equal(h.lastState().status.pending, null);

  // the log cursor survived the pending checks: no line came twice
  const lines = h.log();
  assert.equal(new Set(lines).size, lines.length);

  await h.s.handle({ type: 'cmd', cmd: { cmd: 'double_tap', x: 10, y: 10 } });
  // double_tap delivers a tap first (the confirm opens), then the double tap goes to the dialog
  assert.equal(h.lastState().status.pending.kind, 'confirm');
  assert.equal(h.errors().length, 0);
});

test('a failed first load is a core error, a retry that works posts ready', async () => {
  let n = 0;
  const h = harness({ load: () => (n++ === 0 ? Promise.reject(new Error('no wasm')) : loadCore(MODULE)) });
  await h.s.init();
  assert.equal(h.msgs.at(-1).type, 'error');
  assert.equal(h.msgs.at(-1).kind, 'core');
  await h.s.handle({ type: 'open', files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  assert.equal(h.msgs.filter((m) => m.type === 'ready').length, 1);
  assert.ok(h.lastState());
});

test('the text field replaces an edit\'s prefilled text instead of appending to it', async () => {
  const h = harness();
  const app = files({
    'manifest.json': '{"app_id": "ed", "entry": "index.lua"}',
    'index.lua':
      "msg = 'none'\n" +
      "function on_load(c) c.input:edit({text = 'hi', on_end = function(c, s, how) msg = s .. ':' .. how; c:invalidate() end}) end\n" +
      'function on_draw(c, g) g:clear(0); g:text(0, 0, msg) end\n',
  });
  await h.s.handle({ type: 'open', files: app, fixtures: new Map(), fonts: null, config: config() });
  assert.equal(h.lastState().status.pending.text, 'hi');
  await h.s.handle({ type: 'cmd', cmd: { cmd: 'submit_text', text: 'hi!' }, ifPending: ['text', 'edit'] });
  assert.equal(h.errors().length, 0);
  assert.equal(h.lastState().status.pending, null);
  assert.ok(texts(h.lastState()).includes('hi!:enter'));
});

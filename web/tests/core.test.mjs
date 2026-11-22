// The core's Wasm module through web/js/core.js; build it first with scripts/build-web.sh.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { readFile } from 'node:fs/promises';
import { createHash } from 'node:crypto';

import { loadCore, toBase64 } from '../js/core.js';

const ROOT = new URL('../../', import.meta.url);
const START_LOCAL_MS = 1_795_165_200_000; // 2026-11-20T09:00:00, as crates/inkcore/examples/hello_hash.rs
// crates/inkcore/examples/hello_hash.rs and tests/test_parity.py pin the same value
const HELLO_HASH = 'c8cbd63a6ccc92b539137410455bec2e763b7836ae235d4a2306214079308ccb';

const factory = await loadCore(new URL('../core/inkcore_wasm.js', import.meta.url).href);
const enc = new TextEncoder();

function config(over = {}) {
  return {
    start_local_ms: START_LOCAL_MS,
    utc_offset_sec: 0,
    net: 'replay',
    locale: 'en',
    app_label: 'testapp',
    random_seed: 1,
    ...over,
  };
}

function files(obj) {
  return new Map(Object.entries(obj).map(([k, v]) => [k, typeof v === 'string' ? enc.encode(v) : v]));
}

async function helloFiles() {
  const out = new Map();
  for (const name of ['manifest.json', 'index.lua', 'lockscreen.lua']) {
    out.set(name, new Uint8Array(await readFile(new URL(`examples/hello/${name}`, ROOT))));
  }
  return out;
}

const NET_MANIFEST = '{"app_id": "testapp", "entry": "index.lua", "api": "2.2", "permissions": ["net"]}';

// draws the byte length and the first two bytes of the body the request got
function bodyApp(url) {
  return files({
    'manifest.json': NET_MANIFEST,
    'index.lua':
      "msg = 'none'\n" +
      'function on_draw(c, g) g:clear(0); g:text(0, 0, msg) end\n' +
      'function on_load(c)\n' +
      `  c.net:request({url = '${url}'}, function(c, resp)\n` +
      "    local body = resp.body or ''\n" +
      "    msg = #body .. ':' .. tostring(body:byte(1)) .. ':' .. tostring(body:byte(2))\n" +
      '    c:invalidate()\n' +
      '  end)\n' +
      'end\n',
  });
}

const hasText = (st, s) => st.texts.some((t) => t.text === s);

test('hello: tap counter, then the frame hash pinned in stage 1', async () => {
  const core = factory.create({ files: await helloFiles(), fixtures: new Map(), fonts: null, config: config() });
  try {
    let st = core.cmd({ cmd: 'state' });
    assert.equal(st.ok, true);
    assert.ok(hasText(st, 'Taps: 0'), JSON.stringify(st.texts));
    assert.deepEqual(st.errors, []);
    assert.equal(st.status.app_id, 'hello');
    assert.equal(st.status.time, '2026-11-20T09:00:00.000');
    assert.deepEqual(st.status.screen, [480, 800]);

    // as Core::tap_text: the last item equal to the text, tapped at its middle
    const it = st.texts.findLast((t) => t.text === 'Tap me');
    const tap = core.cmd({ cmd: 'tap', x: it.x + Math.floor(it.w / 2), y: it.y + Math.floor(it.h / 2), with_state: true });
    assert.equal(tap.ok, true);
    assert.ok(hasText(tap, 'Taps: 1'), JSON.stringify(tap.texts));

    assert.deepEqual(core.cmd({ cmd: 'wait', ms: 61_000 }), { ok: true });
    st = core.cmd({ cmd: 'state' });
    assert.deepEqual(st.errors, []);

    const f = core.frame(false);
    assert.equal(f.w, 480);
    assert.equal(f.h, 800);
    assert.equal(f.bits.length, (480 * 800) / 8);
    assert.equal(createHash('sha256').update(f.bits).digest('hex'), HELLO_HASH);
  } finally {
    core.destroy();
  }
});

test('an endless loop in on_load is stopped by the instruction budget', () => {
  const app = files({
    'manifest.json': '{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}',
    'index.lua': 'function on_load(c) while true do end end\nfunction on_draw(c, g) end\n',
  });
  const t0 = performance.now();
  const core = factory.create({ files: app, fixtures: new Map(), fonts: null, config: config() });
  const ms = performance.now() - t0;
  try {
    assert.ok(ms < 5000, `took ${ms} ms`);
    const st = core.cmd({ cmd: 'state' });
    assert.equal(st.errors.length, 1);
    assert.match(st.errors[0], /instruction budget/);
    assert.equal(st.status.running, false);
    assert.ok(st.log.some((l) => l.includes('instruction budget')));
  } finally {
    core.destroy();
  }
});

// needs the module's -sSTACK_SIZE: with emscripten's default 64 KB the C stack overflows and the module dies
test('a C stack overflow is an app error and the module keeps working', () => {
  const app = files({
    'manifest.json': '{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}',
    'index.lua':
      "function on_load(c) local function f(s) return (s:gsub('.', f)) end f('aaaa') end\n" +
      'function on_draw(c, g) end\n',
  });
  const core = factory.create({ files: app, fixtures: new Map(), fonts: null, config: config() });
  try {
    const st = core.cmd({ cmd: 'state' });
    assert.equal(st.errors.length, 1);
    assert.match(st.errors[0], /C stack overflow/);
  } finally {
    core.destroy();
  }
  const again = factory.create({ files: app, fixtures: new Map(), fonts: null, config: config() });
  assert.equal(again.cmd({ cmd: 'state' }).ok, true);
  again.destroy();
});

test('a fixture body with NUL and 0xff reaches Lua byte-exact', () => {
  const fixture = JSON.stringify({
    request: { method: 'GET', url: 'http://x.test/bin' },
    response: { status: 200, body_file: 'bin.body' },
  });
  const fixtures = files({ 'net/bin.json': fixture, 'net/bin.body': new Uint8Array([0, 0xff]) });
  const core = factory.create({ files: bodyApp('http://x.test/bin'), fixtures, fonts: null, config: config() });
  try {
    const st = core.cmd({ cmd: 'wait', ms: 300, with_state: true });
    assert.equal(st.ok, true);
    assert.deepEqual(st.errors, []);
    assert.ok(hasText(st, '2:0:255'), JSON.stringify(st.texts));
  } finally {
    core.destroy();
  }
});

test('fixtures+live: a request without a fixture goes out and the delivered bytes reach Lua', () => {
  const core = factory.create({
    files: bodyApp('http://x.test/live'),
    fixtures: new Map(),
    fonts: null,
    config: config({ net: 'fixtures+live' }),
  });
  try {
    const out = core.cmd({ cmd: 'take_outgoing' });
    assert.equal(out.ok, true);
    assert.equal(out.requests.length, 1);
    const [r] = out.requests;
    assert.equal(r.method, 'GET');
    assert.equal(r.url, 'http://x.test/live');
    assert.equal(r.body_b64, '');
    assert.deepEqual(core.cmd({ cmd: 'take_outgoing' }).requests, []);

    const res = core.cmd({
      cmd: 'deliver',
      id: r.id,
      status: 200,
      headers: [['Content-Type', 'application/octet-stream']],
      body_b64: toBase64(new Uint8Array([0, 0xff])),
      delay_ms: 0,
      with_state: true,
    });
    assert.equal(res.ok, true);
    assert.deepEqual(res.errors, []);
    assert.ok(hasText(res, '2:0:255'), JSON.stringify(res.texts));
    assert.equal(res.status.net, 'fixtures+live');
  } finally {
    core.destroy();
  }
});

test('state: frame_seq moves with a new frame, log lines come from log_from on', () => {
  const app = files({
    'manifest.json': '{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}',
    'index.lua':
      "n = 0\nfunction on_draw(c, g) g:clear(0); g:text(0, 0, 'n ' .. n) end\n" +
      "function on_input(c, ev) if ev.gesture == 'tap' then n = n + 1; c:invalidate() end return true end\n",
  });
  const core = factory.create({ files: app, fixtures: new Map(), fonts: null, config: config() });
  try {
    const a = core.cmd({ cmd: 'state' });
    assert.equal(a.log_from, 0);
    assert.ok(a.log.length > 0);
    const b = core.cmd({ cmd: 'tap', x: 10, y: 10, with_state: true, log_from: a.log.length });
    assert.ok(b.frame_seq > a.frame_seq);
    assert.equal(b.log_from, a.log.length);
    assert.ok(hasText(b, 'n 1'));
    const c = core.cmd({ cmd: 'state', log_from: 10_000 });
    assert.equal(c.frame_seq, b.frame_seq);
    assert.deepEqual(c.log, []);
    assert.equal(c.log_from, a.log.length + b.log.length);
  } finally {
    core.destroy();
  }
});

test('reload with new files runs the new code', () => {
  const app = (text) =>
    files({
      'manifest.json': '{"app_id": "testapp", "entry": "index.lua", "api": "2.2"}',
      'index.lua': `function on_draw(c, g) g:clear(0); g:text(0, 0, '${text}') end\n`,
    });
  const core = factory.create({ files: app('old'), fixtures: new Map(), fonts: null, config: config() });
  try {
    assert.ok(hasText(core.cmd({ cmd: 'state' }), 'old'));
    const r = core.cmd({ cmd: 'reload', files: app('new'), with_state: true });
    assert.equal(r.ok, true, r.error);
    assert.ok(hasText(r, 'new'), JSON.stringify(r.texts));
    assert.ok(hasText(core.cmd({ cmd: 'reload', with_state: true }), 'new'));
  } finally {
    core.destroy();
  }
});

test('bad input gives errors, not a dead module', () => {
  const app = files({ 'manifest.json': '{"app_id": "testapp", "api": "2.2"}', 'index.lua': '' });
  assert.throws(() => factory.create({ files: app, fixtures: new Map(), fonts: null, config: config({ net: 'live' }) }), /net/);
  assert.throws(
    () => factory.create({ files: app, fixtures: new Map(), fonts: [new Uint8Array(1)], config: config() }),
    /fonts/,
  );
  const core = factory.create({ files: app, fixtures: new Map(), fonts: null, config: config() });
  try {
    assert.match(core.cmd({ cmd: 'nope' }).error, /unknown command/);
    assert.match(core.cmd({ cmd: 'tap', x: 'a', y: 1 }).error, /x/);
    assert.match(core.cmd({ cmd: 'key', name: 'zz' }).error, /unknown key/);
    assert.equal(core.cmd({ cmd: 'key', name: 'ok' }).ok, true);
  } finally {
    core.destroy();
  }
  assert.match(core.cmd({ cmd: 'state' }).error, /handle/);
});

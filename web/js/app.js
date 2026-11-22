// The page: loads apps, sends input to the worker, draws its frames. Input semantics follow src/inkemu/window.py.
import { parseGitHubUrl, parseShareParams, shareUrl, resolveAndList, loadGitHubApp } from './github.js';
import { fromZip, fromDirectoryHandle, fromFileList } from './sources.js';

const $ = (id) => document.getElementById(id);
const HOLD_MS = 600;
const MAX_LOG_LINES = 5000;
const KEYMAP = {
  ArrowUp: 'up',
  ArrowDown: 'down',
  ArrowLeft: 'left',
  ArrowRight: 'right',
  Enter: 'ok',
  Escape: 'back',
};
const FONT_SLOTS = ['small', 'medium', 'large'];

const canvas = $('screen');
const g = canvas.getContext('2d');
const logEl = $('log');

let ready = false;
let coreFailed = false;
let loading = false;
let openSeq = 0; // a later open wins over a slower earlier one
let app = null; // {files, fixtures, appId, label, source}
let fonts = null; // [small, medium, large] BDF bytes
let status = null;
let shownPending = '';
let waitingFrame = false;

// ---- log ------------------------------------------------------------------------------------------

function logLine(text, cls) {
  const atBottom = logEl.scrollTop + logEl.clientHeight >= logEl.scrollHeight - 4;
  const div = document.createElement('div');
  div.textContent = text;
  if (cls) div.className = cls;
  else if (/^\S+ ERROR /.test(text)) div.className = 'error';
  else if (/^\S+ WARN /.test(text)) div.className = 'warn';
  logEl.append(div);
  while (logEl.childElementCount > MAX_LOG_LINES) logEl.firstElementChild.remove();
  if (atBottom) logEl.scrollTop = logEl.scrollHeight;
}
const logError = (text) => logLine(text, 'error');
const logPage = (text) => logLine(text, 'page');

function overlay() {
  const el = $('overlay');
  let text = null;
  if (coreFailed) text = 'The emulator core could not be loaded; see the log.';
  else if (!ready) text = 'Loading core…';
  else if (loading) text = 'Loading app…';
  else if (!app) text = 'Open an app: paste a GitHub link to its folder, or pick a zip or a folder.';
  else if (waitingFrame) text = 'Starting…';
  el.textContent = text ?? '';
  el.hidden = text === null;
}

// ---- worker ---------------------------------------------------------------------------------------

const worker = new Worker(new URL('./worker.js', import.meta.url), { type: 'module' });

worker.onmessage = (e) => {
  const m = e.data;
  if (m.type === 'ready') {
    ready = true;
    overlay();
  } else if (m.type === 'frame') {
    draw(m);
    waitingFrame = false;
    overlay();
  } else if (m.type === 'state') {
    onState(m);
  } else if (m.type === 'error') {
    if (!ready && /emulator core/.test(m.message)) coreFailed = true;
    logError(m.message);
    overlay();
  }
};
worker.onerror = (e) => {
  coreFailed = !ready;
  logError(`The emulator worker failed: ${e.message || 'it could not be loaded'}.`);
  overlay();
};

const post = (msg) => worker.postMessage(msg);
const send = (cmd) => app && post({ type: 'cmd', cmd });

function syncTicking() {
  post({ type: 'tick', on: document.visibilityState === 'visible' });
}
document.addEventListener('visibilitychange', syncTicking);
syncTicking();

// ---- screen ---------------------------------------------------------------------------------------

function draw({ w, h, bits }) {
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const img = g.createImageData(w, h);
  const px = new Uint32Array(img.data.buffer);
  const stride = (w + 7) >> 3;
  for (let y = 0; y < h; y++) {
    const row = y * stride;
    for (let x = 0; x < w; x++) {
      // set bit = white (PIL mode "1"); 0xffffffff white, 0xff000000 opaque black in RGBA bytes
      px[y * w + x] = bits[row + (x >> 3)] & (0x80 >> (x & 7)) ? 0xffffffff : 0xff000000;
    }
  }
  g.putImageData(img, 0, 0);
}

function onState(m) {
  for (const line of m.log) logLine(line);
  status = m.status;
  const s = status;
  $('clock').textContent = String(s.time).slice(0, 19).replace('T', ' ');
  const flags = [`net ${s.net}`];
  if (s.paused) flags.push('paused');
  if (s.locked) flags.push('locked');
  if (s.pending) flags.push(`dialog: ${s.pending.kind}`);
  if (!s.running && !s.locked) flags.push('stopped (R to reload)');
  if (m.errors.length) flags.push(`errors ${m.errors.length}`);
  $('flags').textContent = flags.join(' · ');
  $('pause').textContent = s.paused ? 'Resume' : 'Pause';
  $('lock').textContent = s.locked ? 'Unlock' : 'Lock';
  if (app) document.title = `inkemu — ${s.app_id}`;
  showTextField(s.pending);
}

function showTextField(p) {
  const form = $('text-form');
  const isText = p && (p.kind === 'text' || p.kind === 'edit');
  if (!isText) {
    form.hidden = true;
    shownPending = '';
    return;
  }
  const key = JSON.stringify(p);
  if (key === shownPending) return;
  shownPending = key;
  $('text-label').textContent = p.title || 'Input';
  const input = $('text-input');
  input.value = p.text ?? '';
  form.hidden = false;
  input.focus();
  input.select();
}

$('text-form').addEventListener('submit', (e) => {
  e.preventDefault();
  send({ cmd: 'type_text', text: $('text-input').value });
});
$('text-input').addEventListener('keydown', (e) => {
  if (e.key === 'Escape') {
    e.preventDefault();
    send({ cmd: 'answer', ok: false });
  }
});

// ---- pointer: click = tap, drag = swipe, long press = hold (the core classifies on `up`) ----------------

let down = false;
let holdTimer = null;
let lastXY = null;

function xy(e) {
  const r = canvas.getBoundingClientRect();
  const x = Math.floor(((e.clientX - r.left) * canvas.width) / r.width);
  const y = Math.floor(((e.clientY - r.top) * canvas.height) / r.height);
  return [Math.min(Math.max(x, 0), canvas.width - 1), Math.min(Math.max(y, 0), canvas.height - 1)];
}

const pointer = (phase, [x, y]) => send({ cmd: 'pointer', phase, x, y });

canvas.addEventListener('pointerdown', (e) => {
  if (e.button !== 0 || !app) return;
  e.preventDefault();
  // keys go to the screen again after a click on it
  if (inTextField(document.activeElement)) document.activeElement.blur();
  try {
    canvas.setPointerCapture(e.pointerId);
  } catch {
    // a pointer the browser no longer tracks; the moves still arrive while over the canvas
  }
  down = true;
  lastXY = xy(e);
  pointer('down', lastXY);
  const at = lastXY;
  holdTimer = setTimeout(() => {
    holdTimer = null;
    if (down) pointer('hold', at);
  }, HOLD_MS);
});

canvas.addEventListener('pointermove', (e) => {
  if (!down) return;
  const p = xy(e);
  if (p[0] === lastXY[0] && p[1] === lastXY[1]) return;
  lastXY = p;
  pointer('move', p);
});

function release(e) {
  if (!down) return;
  down = false;
  if (holdTimer) {
    clearTimeout(holdTimer);
    holdTimer = null;
  }
  pointer('up', e.type === 'pointerup' ? xy(e) : lastXY);
}
canvas.addEventListener('pointerup', release);
canvas.addEventListener('pointercancel', release);

// ---- keyboard: as the window's on_key ------------------------------------------------------------

function inTextField(el) {
  return el instanceof Element && !!el.closest('input, textarea, select, [contenteditable=""], [contenteditable="true"]');
}

function togglePause() {
  if (status) send({ cmd: 'set_paused', paused: !status.paused });
}
const toggleLock = () => status && send({ cmd: status.locked ? 'unlock' : 'lock' });

document.addEventListener('keydown', (e) => {
  if (e.ctrlKey || e.metaKey || e.altKey || inTextField(e.target)) return;
  // the physical key (any layout), or the character when there is no code (synthetic events)
  const ch = e.key.length === 1 ? e.key.toLowerCase() : '';
  const is = (letter) => e.code === `Key${letter.toUpperCase()}` || ch === letter;
  const plusHour = ch === '}' || (e.code === 'BracketRight' && e.shiftKey);
  const plusMinute = !plusHour && (ch === ']' || e.code === 'BracketRight');
  if (is('p')) togglePause();
  else if (is('s')) savePng();
  else if (!app || !status) return;
  else if (status.pending) {
    // confirm dialogs: Y/N here, or tap the plate's buttons on the screen
    const textKind = status.pending.kind === 'text' || status.pending.kind === 'edit';
    if (is('y') && !textKind) send({ cmd: 'answer', ok: true });
    else if (is('n') && !textKind) send({ cmd: 'answer', ok: false });
    else return;
  } else if (KEYMAP[e.key] || e.code === 'NumpadEnter') send({ cmd: 'key', name: KEYMAP[e.key] ?? 'ok' });
  else if (is('l')) toggleLock();
  else if (is('r')) send({ cmd: 'reload' });
  else if (plusMinute) send({ cmd: 'wait', ms: 60000 });
  else if (plusHour) send({ cmd: 'wait', ms: 3600000 });
  else return;
  e.preventDefault();
});

// ---- controls -------------------------------------------------------------------------------------

for (const b of document.querySelectorAll('[data-wait]')) {
  b.addEventListener('click', () => send({ cmd: 'wait', ms: Number(b.dataset.wait) }));
}
for (const b of document.querySelectorAll('[data-key]')) {
  b.addEventListener('click', () => send({ cmd: 'key', name: b.dataset.key }));
}
for (const b of document.querySelectorAll('[data-cmd]')) {
  b.addEventListener('click', () => send({ cmd: b.dataset.cmd }));
}
$('pause').addEventListener('click', togglePause);
$('lock').addEventListener('click', toggleLock);
$('save-png').addEventListener('click', savePng);
$('log-clear').addEventListener('click', () => logEl.replaceChildren());

function savePng() {
  if (!app) return;
  const t = new Date();
  const p = (n) => String(n).padStart(2, '0');
  const stamp = `${t.getFullYear()}${p(t.getMonth() + 1)}${p(t.getDate())}-${p(t.getHours())}${p(t.getMinutes())}${p(t.getSeconds())}`;
  const name = `${status?.app_id || app.appId}-${stamp}.png`;
  canvas.toBlob((blob) => {
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = name;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
    logPage(`saved ${name}`);
  }, 'image/png');
}

$('net-replay').addEventListener('change', (e) => {
  if (!app) return;
  logPage(e.target.checked ? 'net: fixtures only — the app starts again' : 'net: fixtures, then the network — the app starts again');
  start();
});

$('fonts-btn').addEventListener('click', () => $('fonts-input').click());
$('fonts-input').addEventListener('change', async (e) => {
  const list = [...e.target.files];
  e.target.value = '';
  const pick = FONT_SLOTS.map((slot) => list.find((f) => f.name.toLowerCase().includes(slot)));
  if (list.length !== 3 || pick.some((f) => !f)) {
    logError('Fonts: pick three BDF files whose names contain small, medium and large (small.bdf, medium.bdf, large.bdf).');
    return;
  }
  fonts = await Promise.all(pick.map(async (f) => new Uint8Array(await f.arrayBuffer())));
  logPage(`fonts: ${pick.map((f) => f.name).join(', ')}${app ? ' — the app starts again' : ''}`);
  if (app) start();
});

$('copy-link').addEventListener('click', async () => {
  if (app?.source.kind !== 'gh') return;
  const link = shareUrl(location.href, app.source);
  try {
    await navigator.clipboard.writeText(link);
    logPage(`link copied: ${link}`);
  } catch {
    logPage(`link: ${link}`);
  }
});

// ---- opening apps ---------------------------------------------------------------------------------

function pageLocale() {
  const m = /^([a-z]{2})(?:[-_]|$)/i.exec(navigator.language || '');
  return m ? m[1].toLowerCase() : 'en';
}

function config() {
  const offset = -new Date().getTimezoneOffset() * 60;
  return {
    start_local_ms: Date.now() + offset * 1000,
    utc_offset_sec: offset,
    net: $('net-replay').checked ? 'replay' : 'fixtures+live',
    locale: pageLocale(),
    // the core's fallback app id takes the letters of the label's last segment, as the app folder's name
    app_label: app.appId,
    random_seed: null,
  };
}

/** A fresh core for the current app (new time, net mode, fonts). */
function start() {
  waitingFrame = true;
  status = null;
  shownPending = '';
  $('text-form').hidden = true;
  post({ type: 'open', files: app.files, fixtures: app.fixtures, fonts, config: config() });
  overlay();
}

function showSource() {
  const kind = app?.source.kind;
  $('copy-link').hidden = kind !== 'gh';
  $('reread-btn').hidden = kind !== 'dir';
  $('source-label').textContent = app?.label ?? '';
  if (kind === 'gh') history.replaceState(null, '', shareUrl(location.href, app.source));
  else if (location.search) history.replaceState(null, '', location.pathname);
}

/** `loader()` resolves to {files, fixtures, appId, label, source}. */
async function openApp(loader) {
  const seq = ++openSeq;
  loading = true;
  overlay();
  try {
    const loaded = await loader();
    if (seq !== openSeq) return;
    app = loaded;
    document.title = `inkemu — ${app.appId}`;
    logPage(`— open ${app.label} (${app.files.size} files${app.fixtures.size ? `, ${app.fixtures.size} fixture files` : ''})`);
    showSource();
    start();
  } catch (e) {
    if (seq !== openSeq) return;
    if (e?.name !== 'AbortError') logError(`Could not open the app: ${e?.message ?? e}`);
  } finally {
    if (seq === openSeq) {
      loading = false;
      overlay();
    }
  }
}

const ghLink = ({ owner, repo, ref, path }) => `https://github.com/${owner}/${repo}/tree/${ref}${path ? `/${path}` : ''}`;

$('gh-form').addEventListener('submit', (e) => {
  e.preventDefault();
  const text = $('gh-url').value.trim();
  const parsed = parseGitHubUrl(text);
  if (!parsed) {
    logError(
      `Not a GitHub link to an app folder: ${text || '(empty)'}. Expected https://github.com/<owner>/<repo>/tree/<branch>/<folder> ` +
        'or a link to its manifest.json.',
    );
    return;
  }
  openApp(async () => {
    const { ref, path, tree } = await resolveAndList(parsed);
    const { owner, repo } = parsed;
    const a = await loadGitHubApp({ owner, repo, ref, path, tree });
    return { ...a, source: { kind: 'gh', owner, repo, ref, path } };
  });
});

$('zip-btn').addEventListener('click', () => $('zip-input').click());
$('zip-input').addEventListener('change', (e) => {
  const file = e.target.files[0];
  e.target.value = '';
  if (!file) return;
  openApp(async () => ({ ...(await fromZip(await file.arrayBuffer(), file.name)), source: { kind: 'zip' } }));
});

$('folder-btn').addEventListener('click', async () => {
  if (!window.showDirectoryPicker) {
    $('folder-input').click();
    return;
  }
  let handle;
  try {
    handle = await window.showDirectoryPicker();
  } catch (e) {
    if (e?.name !== 'AbortError') logError(`Could not open the folder: ${e?.message ?? e}`);
    return;
  }
  openApp(async () => ({ ...(await fromDirectoryHandle(handle)), source: { kind: 'dir', handle } }));
});
$('folder-input').addEventListener('change', (e) => {
  const list = [...e.target.files];
  e.target.value = '';
  if (!list.length) return;
  openApp(async () => ({ ...(await fromFileList(list)), source: { kind: 'files' } }));
});

$('reread-btn').addEventListener('click', async () => {
  if (app?.source.kind !== 'dir') return;
  const current = app;
  try {
    const a = await fromDirectoryHandle(current.source.handle);
    if (app !== current) return;
    app = { ...current, files: a.files, fixtures: a.fixtures };
    post({ type: 'reload', files: a.files, fixtures: a.fixtures });
    logPage(`— re-read ${a.label} (${a.files.size} files)`);
  } catch (e) {
    logError(`Could not re-read the folder: ${e?.message ?? e}`);
  }
});

// ---- startup: ?gh=<owner>/<repo>&ref=<ref>&path=<path> ------------------------------------------------

const params = new URLSearchParams(location.search);
if (params.has('gh')) {
  const share = parseShareParams(params);
  if (!share) {
    logError(`The link's app is not valid: gh=${params.get('gh')} (expected ?gh=<owner>/<repo>&ref=<branch>&path=<folder>).`);
  } else {
    $('gh-url').value = ghLink(share);
    openApp(async () => ({ ...(await loadGitHubApp(share)), source: { kind: 'gh', ...share } }));
  }
}
overlay();

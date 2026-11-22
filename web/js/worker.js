// Owns the Wasm core: runs page commands, lets real time pass, performs the app's requests. See protocol.md.
import { loadCore } from './core.js';
import { perform } from './net.js';

const TICK_MS = 100;
const MAX_STEP_MS = 5000; // a sleeping laptop must not fast-forward the app by hours

/**
 * The worker's logic without the worker: `post(msg, transfer)` sends to the page, `load()` returns a core factory
 * (core.js loadCore). Pure enough to run in Node.
 */
export function createSession({ post, load, fetchFn = globalThis.fetch, now = () => performance.now() }) {
  let factory = null; // Promise of the loadCore() result; dropped after a crash so the next start gets a new module
  let core = null;
  let gen = 0; // bumps with every new core, so answers to an old core's requests are dropped
  let last = null; // the last open payload, to start again after a crash
  let logFrom = 0;
  let lastSeq = null;
  let lastKey = null;
  let paused = false;
  let appId = '';
  let clock = '';
  let extra = []; // the worker's own log lines, sent with the next state
  let timer = null;
  let lastTick = now();
  let queue = Promise.resolve();

  const getFactory = () =>
    (factory ??= load().catch((e) => {
      factory = null;
      throw e;
    }));

  const say = (line) => extra.push(clock ? `${clock} ${line}` : line);

  function fatal(e) {
    core = null;
    factory = null;
    gen++;
    const what = e instanceof WebAssembly.RuntimeError ? 'crashed' : 'failed';
    const message = `The emulator core ${what} (${e?.message ?? e}). The app is stopped; press Reload or open the app again to start it fresh.`;
    post({ type: 'error', message });
  }

  function publish(st) {
    if (!Array.isArray(st.log) || !st.status) return;
    logFrom = st.log_from + st.log.length;
    paused = st.status.paused;
    appId = st.status.app_id;
    clock = String(st.status.time).slice(11, 23);
    if (st.frame_seq !== lastSeq) {
      lastSeq = st.frame_seq;
      const f = core.frame(true);
      post({ type: 'frame', w: f.w, h: f.h, bits: f.bits, seq: st.frame_seq }, [f.bits.buffer]);
    }
    const log = extra.length ? [...st.log, ...extra] : st.log;
    extra = [];
    // the clock to the second: a state for every 100 ms tick would only repeat itself
    const key = JSON.stringify([{ ...st.status, time: String(st.status.time).slice(0, 19) }, st.errors.length, st.frame_seq]);
    if (key === lastKey && !log.length) return;
    lastKey = key;
    post({ type: 'state', status: st.status, texts: st.texts, errors: st.errors, log });
  }

  function drain() {
    const out = core.cmd({ cmd: 'take_outgoing' });
    for (const req of out.requests ?? []) {
      const g = gen;
      perform(req, fetchFn).then((res) => {
        if (g !== gen || !core) return;
        const { log, ...args } = res;
        if (log) say(log.replace('<app_id>', appId || '<app_id>'));
        run({ cmd: 'deliver', ...args });
      });
    }
  }

  /** One core command with the state merged in; then the outbox, the frame and the state go out. */
  function run(cmd) {
    if (!core) return null;
    try {
      const r = core.cmd({ ...cmd, with_state: true, log_from: logFrom });
      if (!r.ok) post({ type: 'error', message: `${cmd.cmd}: ${r.error}` });
      drain();
      publish(r);
      return r;
    } catch (e) {
      fatal(e);
      return null;
    }
  }

  /** The kind of the dialog open in the core right now, or null; the page's last state may be behind. */
  function pendingKind() {
    try {
      // log_from past the end: no log lines to account for, the cursor stays
      return core.cmd({ cmd: 'state', log_from: Number.MAX_SAFE_INTEGER }).status?.pending?.kind ?? null;
    } catch (e) {
      fatal(e);
      return null;
    }
  }

  /** Real time since the last tick reaches the core (the window's 1:1 clock). */
  function catchUp() {
    const t = now();
    if (!core || paused) {
      lastTick = t;
      return;
    }
    const elapsed = t - lastTick;
    let ms = Math.floor(elapsed);
    if (elapsed > MAX_STEP_MS) {
      ms = MAX_STEP_MS;
      lastTick = t;
    } else {
      lastTick += ms; // keep the fraction for the next tick
    }
    if (ms > 0) run({ cmd: 'advance', ms });
  }

  async function start() {
    const f = await getFactory();
    try {
      core?.destroy();
    } catch {
      // a crashed module: nothing to free
    }
    core = null;
    gen++;
    logFrom = 0;
    lastSeq = null;
    lastKey = null;
    extra = [];
    clock = '';
    lastTick = now();
    try {
      core = f.create(last);
    } catch (e) {
      if (e instanceof WebAssembly.RuntimeError) return fatal(e);
      post({ type: 'error', message: `Could not start the app: ${e?.message ?? e}` });
      return;
    }
    run({ cmd: 'state' });
  }

  async function handle(msg) {
    switch (msg.type) {
      case 'open':
        last = { files: msg.files, fixtures: msg.fixtures, fonts: msg.fonts ?? null, config: msg.config };
        return start();
      case 'reload':
        if (!last) return post({ type: 'error', message: 'No app is open.' });
        last = { ...last, files: msg.files, ...(msg.fixtures ? { fixtures: msg.fixtures } : {}) };
        if (!core) return start();
        run({ cmd: 'reload', files: msg.files, ...(msg.fixtures ? { fixtures: msg.fixtures } : {}) });
        return;
      case 'cmd': {
        const { cmd } = msg;
        if (!core) {
          if (last && (cmd.cmd === 'reload' || cmd.cmd === 'restart')) return start();
          return post({ type: 'error', message: last ? 'The app is stopped; press Reload.' : 'No app is open.' });
        }
        if (msg.ifPending && !msg.ifPending.includes(pendingKind())) return;
        // as the window's on_release: real time must reach the clock before `up` classifies the gesture
        if (cmd.cmd === 'pointer' || cmd.cmd === 'double_tap') catchUp();
        if (cmd.cmd === 'set_paused') lastTick = now();
        run(cmd);
        return;
      }
      case 'tick':
        if (msg.on && timer === null) {
          lastTick = now();
          timer = setInterval(catchUp, TICK_MS);
        } else if (!msg.on && timer !== null) {
          clearInterval(timer);
          timer = null;
        }
        return;
      default:
        post({ type: 'error', message: `unknown message ${msg.type}` });
    }
  }

  return {
    /** Loads the module; posts `ready` or an error. */
    init() {
      return getFactory().then(
        () => post({ type: 'ready' }),
        (e) => post({ type: 'error', message: `Could not load the emulator core: ${e?.message ?? e}` }),
      );
    },
    /** Messages run one at a time, in order (open waits for the module). */
    handle(msg) {
      queue = queue.then(() => handle(msg)).catch((e) => post({ type: 'error', message: String(e?.message ?? e) }));
      return queue;
    },
    tick: catchUp,
  };
}

if (typeof WorkerGlobalScope !== 'undefined' && self instanceof WorkerGlobalScope) {
  const session = createSession({
    post: (msg, transfer) => self.postMessage(msg, transfer ?? []),
    load: () => loadCore(new URL('../core/inkcore_wasm.js', import.meta.url).href),
  });
  self.onmessage = (e) => session.handle(e.data);
  session.init();
}

// The core's Wasm module (web/core/inkcore_wasm.js) behind a small JS API. Pure: runs in a worker and in Node.

const FONT_NAMES = ['small.bdf', 'medium.bdf', 'large.bdf'];
const enc = new TextEncoder();
const dec = new TextDecoder();

export function toBase64(bytes) {
  let s = '';
  for (let i = 0; i < bytes.length; i += 0x8000) {
    s += String.fromCharCode.apply(null, bytes.subarray(i, i + 0x8000));
  }
  return btoa(s);
}

export function fromBase64(text) {
  const s = atob(text);
  const out = new Uint8Array(s.length);
  for (let i = 0; i < s.length; i++) out[i] = s.charCodeAt(i);
  return out;
}

/** Repeated `[u32 LE name_len][name][u32 LE len][bytes]` for `app/`, `fixtures/` and `fonts/` entries. */
function encodeBlob({ files, fixtures, fonts }) {
  const parts = [];
  const add = (name, data) => {
    const n = enc.encode(name);
    const head = new DataView(new ArrayBuffer(4));
    head.setUint32(0, n.length, true);
    parts.push(new Uint8Array(head.buffer), n);
    const len = new DataView(new ArrayBuffer(4));
    len.setUint32(0, data.length, true);
    parts.push(new Uint8Array(len.buffer), data);
  };
  for (const [path, data] of files ?? []) add(`app/${path}`, data);
  for (const [path, data] of fixtures ?? []) add(`fixtures/${path}`, data);
  (fonts ?? []).forEach((data, i) => add(`fonts/${FONT_NAMES[i] ?? i}`, data));
  const out = new Uint8Array(parts.reduce((n, p) => n + p.length, 0));
  let o = 0;
  for (const p of parts) {
    out.set(p, o);
    o += p.length;
  }
  return out;
}

export async function loadCore(moduleUrl) {
  const { default: init } = await import(moduleUrl);
  const M = await init();

  // memory growth replaces the heap: read M.HEAPU8 afresh after every call into the module
  const withBytes = (bytes, f) => {
    const ptr = M._malloc(Math.max(bytes.length, 1));
    M.HEAPU8.set(bytes, ptr);
    try {
      return f(ptr, bytes.length);
    } finally {
      M._free(ptr);
    }
  };
  const resultText = (ptr) => dec.decode(M.HEAPU8.slice(ptr, ptr + M._ik_result_len()));

  class Core {
    #h;

    constructor(handle) {
      this.#h = handle;
    }

    /** One JSON command; `reload` also takes `files` (and `fixtures`) Maps in place of `blob`. */
    cmd(obj) {
      if (obj.cmd === 'reload' && obj.files) {
        const { files, fixtures, ...rest } = obj;
        obj = { ...rest, blob: toBase64(encodeBlob({ files, fixtures })) };
      }
      const ptr = withBytes(enc.encode(JSON.stringify(obj)), (p, n) => M._ik_cmd(this.#h, p, n));
      return JSON.parse(resultText(ptr));
    }

    /** PIL mode "1" (rows of ceil(w / 8) bytes, MSB first, set bit = white): the app's own frame, or with
     * every plate (error, dialog, toast) when `dialog` is true. `gray` is a gray frame in PIL mode "L" (w * h
     * bytes: 255, 170, 85, 0 for 0 to 3 ink steps), else null; `bits` is then its 1-bit threshold. */
    frame(dialog) {
      const ptr = M._ik_frame(this.#h, dialog ? 1 : 0);
      if (!ptr) throw new Error(`unknown core handle ${this.#h}`);
      const w = M._ik_frame_w(this.#h);
      const h = M._ik_frame_h(this.#h);
      const bits = M.HEAPU8.slice(ptr, ptr + Math.ceil(w / 8) * h);
      const gp = M._ik_frame_gray(this.#h);
      return { w, h, bits, gray: gp ? M.HEAPU8.slice(gp, gp + w * h) : null };
    }

    destroy() {
      M._ik_destroy(this.#h);
    }
  }

  return {
    /** `files`/`fixtures`: Map<path, Uint8Array>; `fonts`: [small, medium, large] BDF bytes or null. */
    create({ files, fixtures, fonts, config }) {
      const blob = encodeBlob({ files, fixtures, fonts });
      const handle = withBytes(blob, (bp, bn) =>
        withBytes(enc.encode(JSON.stringify(config)), (cp, cn) => M._ik_create(bp, bn, cp, cn)),
      );
      if (!handle) throw new Error(resultText(M._ik_last_error()));
      return new Core(handle);
    },
  };
}

// Turns a zip, a picked folder or a file list into the Maps that core.js create() takes.
import { unzipSync } from '../vendor/fflate.mjs';

export const MAX_BYTES = 20 * 1024 * 1024;
export const MAX_FILES = 2000;

export class TooLarge extends Error {
  constructor(bytes) {
    super(`The app is ${(bytes / 1048576).toFixed(1)} MiB; the limit is 20 MiB.`);
    this.name = 'TooLarge';
    this.kind = 'too_large';
    this.bytes = bytes;
  }
}

export class TooManyFiles extends Error {
  constructor(count) {
    super(`The app has ${count} files; the limit is 2000.`);
    this.name = 'TooManyFiles';
    this.kind = 'too_many_files';
    this.count = count;
  }
}

export class NotAnApp extends Error {
  constructor(message) {
    super(message);
    this.name = 'NotAnApp';
    this.kind = 'not_an_app';
  }
}

const dec = new TextDecoder();
const SKIP = /(^|\/)(\.git|node_modules|__MACOSX)\//;

const dirOf = (p) => (p.includes('/') ? p.slice(0, p.lastIndexOf('/')) : '');
const depth = (dir) => (dir === '' ? 0 : dir.split('/').length);

/** Shallowest folder holding manifest.json ('' = the root), or null. */
export function findAppRoot(paths) {
  let best = null;
  for (const p of paths) {
    if (SKIP.test(p) || (p !== 'manifest.json' && !p.endsWith('/manifest.json'))) continue;
    const dir = dirOf(p);
    if (best === null || depth(dir) < depth(best) || (depth(dir) === depth(best) && dir < best)) best = dir;
  }
  return best;
}

/** The nearest `tests/fixtures/<appId>/` walking up from appDir; returns it without a trailing slash. */
export function findFixtures(allPaths, appDir, appId) {
  if (!appId) return null;
  let dir = appDir;
  for (;;) {
    const fx = `${dir ? `${dir}/` : ''}tests/fixtures/${appId}`;
    if (allPaths.some((p) => p.startsWith(`${fx}/`))) return fx;
    if (dir === '') return null;
    dir = dirOf(dir);
  }
}

export function readAppId(manifestBytes) {
  try {
    const id = JSON.parse(dec.decode(manifestBytes)).app_id;
    return typeof id === 'string' && id ? id : null;
  } catch {
    return null;
  }
}

const isFixturePath = (rel) => rel === 'reader.json' || rel.startsWith('net/');

/**
 * Picks the app folder, its fixtures and enforces the limits, before any file body is read.
 * `sizes`: Map<path, bytes>; `readFile(path)` -> Uint8Array of one file.
 */
export async function planApp({ sizes, appDir, readFile }) {
  const paths = [...sizes.keys()].filter((p) => !SKIP.test(p));
  if (appDir === undefined) {
    appDir = findAppRoot(paths);
    if (appDir === null) throw new NotAnApp('No manifest.json found: this is not an app folder.');
  }
  const pre = appDir ? `${appDir}/` : '';
  if (!sizes.has(`${pre}manifest.json`)) throw new NotAnApp(`No manifest.json in ${appDir || 'the root folder'}.`);
  const manifestSize = sizes.get(`${pre}manifest.json`);
  if (manifestSize > MAX_BYTES) throw new TooLarge(manifestSize);
  const appId = readAppId(await readFile(`${pre}manifest.json`));
  const fxDir = findFixtures(paths, appDir, appId);
  const fxPre = fxDir ? `${fxDir}/` : null;

  const app = []; // [key, path]
  const fix = [];
  for (const p of paths) {
    if (fxPre && p.startsWith(fxPre)) {
      const rel = p.slice(fxPre.length);
      if (isFixturePath(rel)) fix.push([rel, p]);
    } else if (p.startsWith(pre)) {
      app.push([p.slice(pre.length), p]);
    }
  }
  const count = app.length + fix.length;
  if (count > MAX_FILES) throw new TooManyFiles(count);
  let bytes = 0;
  for (const [, p] of [...app, ...fix]) bytes += sizes.get(p);
  if (bytes > MAX_BYTES) throw new TooLarge(bytes);
  return { appDir, appId: appId ?? (appDir.split('/').pop() || 'app'), app, fix };
}

/** Reads a plan's files with at most `limit` reads in flight. */
export async function readPlan(plan, readFile, limit = 8) {
  const files = new Map();
  const fixtures = new Map();
  const jobs = [...plan.app.map((e) => [files, ...e]), ...plan.fix.map((e) => [fixtures, ...e])];
  let next = 0;
  let failed = false;
  const worker = async () => {
    while (next < jobs.length && !failed) {
      const [map, key, path] = jobs[next++];
      try {
        map.set(key, await readFile(path));
      } catch (e) {
        failed = true;
        throw e;
      }
    }
  };
  await Promise.all(Array.from({ length: Math.min(limit, jobs.length) }, worker));
  return { files, fixtures };
}

async function assemble(sizes, readFile, label) {
  const plan = await planApp({ sizes, readFile });
  const { files, fixtures } = await readPlan(plan, readFile);
  return { files, fixtures, appId: plan.appId, label };
}

/** `name` is only for the label. */
export async function fromZip(arrayBuffer, name) {
  const data = arrayBuffer instanceof Uint8Array ? arrayBuffer : new Uint8Array(arrayBuffer);
  const sizes = new Map();
  unzipSync(data, {
    filter(f) {
      const unsafe = f.name.startsWith('/') || /^[A-Za-z]:/.test(f.name) || f.name.split('/').includes('..');
      if (!unsafe && !f.name.endsWith('/')) sizes.set(f.name, f.originalSize);
      return false;
    },
  });
  const unzip = (want) => unzipSync(data, { filter: (f) => want.has(f.name) });
  const plan = await planApp({ sizes, readFile: async (p) => unzip(new Set([p]))[p] });
  const out = unzip(new Set([...plan.app, ...plan.fix].map((e) => e[1])));
  const { files, fixtures } = await readPlan(plan, async (p) => out[p]);
  return { files, fixtures, appId: plan.appId, label: name ? `zip: ${name}` : 'zip' };
}

export async function fromDirectoryHandle(handle) {
  const handles = new Map();
  const sizes = new Map();
  async function walk(dir, prefix) {
    for await (const [name, h] of dir.entries()) {
      if (name === '.git' || name === 'node_modules') continue;
      if (h.kind === 'directory') {
        await walk(h, `${prefix}${name}/`);
      } else {
        const file = await h.getFile();
        handles.set(prefix + name, file);
        sizes.set(prefix + name, file.size);
      }
    }
  }
  await walk(handle, '');
  const read = async (p) => new Uint8Array(await handles.get(p).arrayBuffer());
  return assemble(sizes, read, `folder: ${handle.name}`);
}

export async function fromFileList(fileList) {
  const byPath = new Map();
  for (const f of fileList) byPath.set(f.webkitRelativePath || f.name, f);
  const sizes = new Map([...byPath].map(([p, f]) => [p, f.size]));
  const first = [...byPath.keys()][0] ?? '';
  const read = async (p) => new Uint8Array(await byPath.get(p).arrayBuffer());
  return assemble(sizes, read, `folder: ${first.split('/')[0] || 'upload'}`);
}

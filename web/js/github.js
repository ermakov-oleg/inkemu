// GitHub apps without a token: one trees API call, files from raw.githubusercontent.com.
import { planApp, readPlan, NotAnApp } from './sources.js';

export class GitHubError extends Error {
  constructor(kind, message) {
    super(message);
    this.name = 'GitHubError';
    this.kind = kind;
  }
}
export class RateLimited extends GitHubError {
  constructor(resetAt) {
    const when = resetAt ? ` Try again after ${resetAt.toLocaleTimeString()}.` : '';
    super('rate_limited', `GitHub API rate limit reached (60 requests per hour without a token).${when}`);
    this.name = 'RateLimited';
    this.resetAt = resetAt;
  }
}
export class NotFound extends GitHubError {
  constructor(what) {
    super('not_found', `Not found on GitHub: ${what} (a private repository or a wrong link?).`);
    this.name = 'NotFound';
  }
}
export class Truncated extends GitHubError {
  constructor() {
    super('truncated', 'The repository is too large for the GitHub tree API (listing truncated). Open a smaller repository or upload a zip of the app.');
    this.name = 'Truncated';
  }
}

export class NetworkError extends GitHubError {
  constructor(timedOut, cause) {
    super('network', timedOut
      ? 'Could not reach GitHub: the request timed out.'
      : 'Could not reach GitHub (offline, blocked, or a network error). Check the connection and try again.');
    this.name = 'NetworkError';
    this.cause = cause;
  }
}

/** `handle(res)` runs under the same timeout, so a stalled body ends as "timed out" too. */
async function request(fetchFn, url, opts, timeoutMs = 30000, handle = async (r) => r) {
  // Not AbortSignal.timeout: its timer is unref'd and would not keep Node alive.
  const ctl = new AbortController();
  const timer = setTimeout(() => ctl.abort(), timeoutMs);
  try {
    // the race also ends a body read that ignores the abort
    const aborted = new Promise((_, reject) => ctl.signal.addEventListener('abort', () => reject(new Error('aborted'))));
    aborted.catch(() => {});
    return await Promise.race([aborted, (async () => handle(await fetchFn(url, { ...opts, signal: ctl.signal })))()]);
  } catch (e) {
    if (e instanceof GitHubError) throw e;
    throw new NetworkError(ctl.signal.aborted, e);
  } finally {
    clearTimeout(timer);
  }
}

const NAME = /^[A-Za-z0-9_.-]+$/;
const dec = (s) => {
  try {
    return decodeURIComponent(s);
  } catch {
    return s;
  }
};
const encPath = (p) => p.split('/').map(encodeURIComponent).join('/');
const DOTS = (segs) => segs.some((x) => x === '.' || x === '..');
const MANIFEST = /(^|\/)manifest\.json$/;

export function parseGitHubUrl(text) {
  const m = /^(?:https?:\/\/)?(?:www\.)?github\.com\/(.+)$/i.exec(String(text).trim().split(/[?#]/)[0]);
  if (!m) return null;
  const seg = m[1].split('/').filter(Boolean).map(dec);
  if (seg.length < 2 || DOTS(seg)) return null;
  const owner = seg[0];
  const repo = seg[1].replace(/\.git$/, '');
  if (!NAME.test(owner) || !NAME.test(repo)) return null;
  if (seg.length === 2) return { owner, repo, refAndPath: [], kind: 'root' };
  const kind = seg[2];
  const rest = seg.slice(3);
  if (kind === 'tree' && rest.length >= 1) return { owner, repo, refAndPath: rest, kind };
  if (kind === 'blob' && rest.length >= 2 && rest[rest.length - 1] === 'manifest.json') {
    return { owner, repo, refAndPath: rest, kind };
  }
  return null;
}

export function parseShareParams(params) {
  const gh = params.get('gh');
  const m = gh && /^([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+)$/.exec(gh);
  if (!m || m[1] === '.' || m[1] === '..' || m[2] === '.' || m[2] === '..') return null;
  const ref = params.get('ref') || 'HEAD';
  const path = params.get('path') || '';
  if (DOTS(ref.split('/')) || DOTS(path.split('/'))) return null;
  return { owner: m[1], repo: m[2], ref, path };
}

export function shareUrl(base, { owner, repo, ref, path }) {
  const q = (s) => encodeURIComponent(s).replace(/%2F/gi, '/');
  let url = `${base.split(/[?#]/)[0]}?gh=${q(`${owner}/${repo}`)}&ref=${q(ref)}`;
  if (path) url += `&path=${q(path)}`;
  return url;
}

async function fetchTree(owner, repo, ref, fetchFn, timeoutMs) {
  const url = `https://api.github.com/repos/${owner}/${repo}/git/trees/${encPath(ref)}?recursive=1`;
  const body = await request(fetchFn, url, { headers: { Accept: 'application/vnd.github+json' } }, timeoutMs, async (res) => {
    if (res.status === 404) return null;
    if ((res.status === 403 || res.status === 429) && res.headers.get('x-ratelimit-remaining') === '0') {
      const reset = Number(res.headers.get('x-ratelimit-reset'));
      throw new RateLimited(reset ? new Date(reset * 1000) : null);
    }
    if (!res.ok) throw new GitHubError('http', `GitHub API answered ${res.status} for ${owner}/${repo}.`);
    return res.json();
  });
  if (!body) return null;
  if (body.truncated) throw new Truncated();
  return body.tree.filter((e) => e.type === 'blob').map((e) => ({ path: e.path, size: e.size ?? 0 }));
}

export async function resolveAndList({ owner, repo, refAndPath }, fetchFn = globalThis.fetch, { timeoutMs } = {}) {
  // Git forbids refs `a` and `a/b` together, so the shortest existing prefix is the ref (3 calls at most).
  if (refAndPath.length === 0) {
    const tree = await fetchTree(owner, repo, 'HEAD', fetchFn, timeoutMs);
    if (tree) return { ref: 'HEAD', path: '', tree };
  }
  for (let n = 1; n <= Math.min(refAndPath.length, 3); n++) {
    const ref = refAndPath.slice(0, n).join('/');
    const tree = await fetchTree(owner, repo, ref, fetchFn, timeoutMs);
    if (tree) return { ref, path: refAndPath.slice(n).join('/'), tree };
  }
  throw new NotFound(`${owner}/${repo}${refAndPath.length ? `@${refAndPath.join('/')}` : ''}`);
}

/** `tree` (from resolveAndList) is optional and saves the second trees call. */
export async function loadGitHubApp({ owner, repo, ref, path, tree }, fetchFn = globalThis.fetch, { timeoutMs } = {}) {
  path = (path || '').replace(/^\/+|\/+$/g, '');
  if (!tree) {
    tree = await fetchTree(owner, repo, ref, fetchFn, timeoutMs);
    if (!tree) throw new NotFound(`${owner}/${repo}@${ref}`);
  }
  const sizes = new Map(tree.map((e) => [e.path, e.size]));
  const manifestIn = (dir) => `${dir ? `${dir}/` : ''}manifest.json`;
  // A blob link points at manifest.json: the app is its folder.
  let appDir = path;
  if (!sizes.has(manifestIn(appDir)) && MANIFEST.test(path) && sizes.has(path)) {
    appDir = path.includes('/') ? path.slice(0, path.lastIndexOf('/')) : '';
  }
  if (!sizes.has(manifestIn(appDir))) {
    const found = [...sizes.keys()].filter((p) => MANIFEST.test(p)).map((p) => p.replace(/\/?manifest\.json$/, '') || '/');
    const hint = found.length ? ` Folders with a manifest.json: ${found.slice(0, 5).join(', ')}.` : '';
    throw new NotAnApp(`No manifest.json in ${owner}/${repo}/${appDir}: this is not an app folder.${hint}`);
  }
  const raw = (p) => `https://raw.githubusercontent.com/${owner}/${repo}/${encPath(ref)}/${encPath(p)}`;
  const readFile = (p) =>
    request(fetchFn, raw(p), {}, timeoutMs, async (res) => {
      if (!res.ok) throw new GitHubError('http', `Could not download ${p} (HTTP ${res.status}).`);
      return new Uint8Array(await res.arrayBuffer());
    });
  const plan = await planApp({ sizes, appDir, readFile });
  const { files, fixtures } = await readPlan(plan, readFile);
  return { files, fixtures, appId: plan.appId, label: `${owner}/${repo}/${appDir}@${ref}` };
}

import { test } from 'node:test';
import assert from 'node:assert/strict';
import {
  parseGitHubUrl, parseShareParams, shareUrl, resolveAndList, loadGitHubApp, RateLimited, NotFound, Truncated,
} from '../js/github.js';
import { TooManyFiles, NotAnApp } from '../js/sources.js';

const enc = new TextEncoder();
const bytes = (s) => (typeof s === 'string' ? enc.encode(s) : s);

/** repos: { 'o/r': { branches: { name: {path: content} } } }; returns fetch + call log. */
function mockGitHub(branches, { rate, truncated } = {}) {
  const calls = [];
  const resp = (status, body, headers = {}) => ({
    status,
    ok: status >= 200 && status < 300,
    headers: { get: (k) => headers[k.toLowerCase()] ?? null },
    json: async () => body,
    arrayBuffer: async () => body,
  });
  const fetchFn = async (url) => {
    calls.push(url);
    if (rate) return resp(403, {}, { 'x-ratelimit-remaining': '0', 'x-ratelimit-reset': '1800000000' });
    let m = /^https:\/\/api\.github\.com\/repos\/o\/r\/git\/trees\/(.+)\?recursive=1$/.exec(url);
    if (m) {
      const ref = decodeURIComponent(m[1]).replace('HEAD', 'main');
      const files = branches[ref];
      if (!files) return resp(404, {});
      const tree = Object.entries(files).map(([path, c]) => ({ path, type: 'blob', size: bytes(c).length }));
      tree.push({ path: 'apps', type: 'tree' });
      return resp(200, { tree, truncated: !!truncated });
    }
    m = /^https:\/\/raw\.githubusercontent\.com\/o\/r\/(.+)$/.exec(url);
    if (m) {
      const rest = decodeURIComponent(m[1]).replace(/%2F/g, '/');
      const br = Object.keys(branches).find((b) => rest.startsWith(`${b}/`));
      const c = br && branches[br][rest.slice(br.length + 1)];
      if (c === undefined || c === null) return resp(404, null);
      const b = bytes(c);
      return resp(200, b.buffer.slice(b.byteOffset, b.byteOffset + b.byteLength));
    }
    throw new Error(`unexpected ${url}`);
  };
  return { fetchFn, calls };
}

const MANIFEST = '{"app_id":"demo"}';
const repo = () => ({
  main: { 'manifest.json': MANIFEST },
  'feature/x': {
    'apps/foo/manifest.json': MANIFEST,
    'apps/foo/index.lua': 'print(1)',
    'apps/foo/bin': new Uint8Array([0, 255]),
    'tests/fixtures/demo/net/GET_a.json': '{}',
    'tests/fixtures/demo/reader.json': '{}',
    'tests/fixtures/demo/notes.txt': 'n',
  },
});

test('parseGitHubUrl', () => {
  assert.deepEqual(parseGitHubUrl('https://github.com/o/r'), { owner: 'o', repo: 'r', refAndPath: [], kind: 'root' });
  assert.deepEqual(parseGitHubUrl('github.com/o/r/'), { owner: 'o', repo: 'r', refAndPath: [], kind: 'root' });
  assert.deepEqual(parseGitHubUrl('https://github.com/o/r/tree/feature/x/apps/foo'),
    { owner: 'o', repo: 'r', refAndPath: ['feature', 'x', 'apps', 'foo'], kind: 'tree' });
  assert.equal(parseGitHubUrl('https://github.com/o/r/blob/main/apps/foo/manifest.json').kind, 'blob');
  assert.equal(parseGitHubUrl('https://github.com/o/r/blob/main/apps/foo/index.lua'), null);
  assert.equal(parseGitHubUrl('https://github.com/o/r/issues/1'), null);
  assert.equal(parseGitHubUrl('https://gitlab.com/o/r'), null);
  assert.equal(parseGitHubUrl('https://github.com/o'), null);
});

test('share URL round trip with slashes', () => {
  const p = { owner: 'o', repo: 'r.js', ref: 'feature/x', path: 'apps/foo bar' };
  const url = shareUrl('https://x.github.io/inkemu/?old=1#h', p);
  assert.ok(url.startsWith('https://x.github.io/inkemu/?gh=o/r.js&ref=feature/x&path=apps/foo%20bar'));
  assert.deepEqual(parseShareParams(new URL(url).searchParams), p);
  assert.equal(parseShareParams(new URLSearchParams('ref=x')), null);
});

test('resolves a branch with a slash through the longer prefix', async () => {
  const { fetchFn, calls } = mockGitHub(repo());
  const r = await resolveAndList({ owner: 'o', repo: 'r', refAndPath: ['feature', 'x', 'apps', 'foo'] }, fetchFn);
  assert.equal(r.ref, 'feature/x');
  assert.equal(r.path, 'apps/foo');
  assert.equal(calls.length, 2);
});

test('unknown ref is NotFound after at most 3 calls', async () => {
  const { fetchFn, calls } = mockGitHub(repo());
  await assert.rejects(resolveAndList({ owner: 'o', repo: 'r', refAndPath: ['a', 'b', 'c', 'd', 'e'] }, fetchFn), NotFound);
  assert.equal(calls.length, 3);
});

test('loads an app with fixtures two levels up', async () => {
  const { fetchFn } = mockGitHub(repo());
  const r = await loadGitHubApp({ owner: 'o', repo: 'r', ref: 'feature/x', path: 'apps/foo' }, fetchFn);
  assert.equal(r.appId, 'demo');
  assert.equal(r.label, 'o/r/apps/foo@feature/x');
  assert.deepEqual([...r.files.keys()].sort(), ['bin', 'index.lua', 'manifest.json']);
  assert.deepEqual([...r.files.get('bin')], [0, 255]);
  assert.deepEqual([...r.fixtures.keys()].sort(), ['net/GET_a.json', 'reader.json']);
});

test('a blob link to manifest.json picks its folder', async () => {
  const { fetchFn } = mockGitHub(repo());
  const l = await resolveAndList({ owner: 'o', repo: 'r', refAndPath: ['feature', 'x', 'apps', 'foo', 'manifest.json'] }, fetchFn);
  const r = await loadGitHubApp({ owner: 'o', repo: 'r', ...l }, fetchFn);
  assert.ok(r.files.has('index.lua'));
});

test('repo root without a manifest is a clear error', async () => {
  const { fetchFn } = mockGitHub({ main: { 'readme.md': 'x', 'apps/a/manifest.json': MANIFEST } });
  const l = await resolveAndList({ owner: 'o', repo: 'r', refAndPath: [] }, fetchFn);
  assert.equal(l.ref, 'HEAD');
  await assert.rejects(loadGitHubApp({ owner: 'o', repo: 'r', ...l }, fetchFn), (e) => e instanceof NotAnApp && /apps\/a/.test(e.message));
});

test('rate limit carries the reset time', async () => {
  const { fetchFn } = mockGitHub(repo(), { rate: true });
  await assert.rejects(resolveAndList({ owner: 'o', repo: 'r', refAndPath: ['main'] }, fetchFn),
    (e) => e instanceof RateLimited && e.resetAt.getTime() === 1800000000 * 1000);
});

test('truncated tree', async () => {
  const { fetchFn } = mockGitHub(repo(), { truncated: true });
  await assert.rejects(resolveAndList({ owner: 'o', repo: 'r', refAndPath: ['main'] }, fetchFn), Truncated);
});

test('2001 files is TooManyFiles and nothing is downloaded', async () => {
  const files = { 'manifest.json': MANIFEST };
  for (let i = 0; i < 2000; i++) files[`f${i}`] = 'x';
  const { fetchFn, calls } = mockGitHub({ main: files });
  await assert.rejects(loadGitHubApp({ owner: 'o', repo: 'r', ref: 'main', path: '' }, fetchFn), TooManyFiles);
  assert.equal(calls.filter((u) => u.includes('raw.')).length, 1); // only the manifest
});

test('downloads at most 8 files at a time', async () => {
  const files = { 'manifest.json': MANIFEST };
  for (let i = 0; i < 40; i++) files[`f${i}`] = 'x';
  const { fetchFn } = mockGitHub({ main: files });
  let live = 0;
  let peak = 0;
  const slow = async (url, o) => {
    if (!url.includes('raw.')) return fetchFn(url, o);
    live++;
    peak = Math.max(peak, live);
    await new Promise((r) => setTimeout(r, 2));
    live--;
    return fetchFn(url, o);
  };
  const r = await loadGitHubApp({ owner: 'o', repo: 'r', ref: 'main', path: '' }, slow);
  assert.equal(r.files.size, 41);
  assert.ok(peak <= 8 && peak > 1, `peak ${peak}`);
});

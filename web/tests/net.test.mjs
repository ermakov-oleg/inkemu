import { test } from 'node:test';
import assert from 'node:assert/strict';
import { perform, corsLine } from '../js/net.js';
import { toBase64, fromBase64 } from '../js/core.js';

const req = (over = {}) => ({
  id: 7,
  method: 'POST',
  url: 'https://api.example.com/x',
  headers: [['content-type', 'application/octet-stream']],
  body_b64: toBase64(new Uint8Array([0, 255, 1])),
  timeout_ms: 5000,
  ...over,
});

const response = (status, bytes, headers) => ({
  status,
  headers: new Headers(headers),
  arrayBuffer: async () => bytes.buffer.slice(bytes.byteOffset, bytes.byteOffset + bytes.byteLength),
});

test('success: the binary body goes out and comes back byte-exact through base64', async () => {
  let seen;
  const fetchFn = async (url, init) => {
    seen = { url, ...init };
    return response(201, new Uint8Array([0, 255, 128, 10]), { 'X-Thing': 'a', 'content-type': 'image/png' });
  };
  const r = await perform(req(), fetchFn);
  assert.equal(seen.url, 'https://api.example.com/x');
  assert.equal(seen.method, 'POST');
  assert.deepEqual(seen.headers, [['content-type', 'application/octet-stream']]);
  assert.deepEqual([...seen.body], [0, 255, 1]);
  assert.ok(seen.signal);
  assert.equal(r.id, 7);
  assert.equal(r.status, 201);
  assert.equal(r.delay_ms, 0);
  assert.equal(r.error, undefined);
  assert.equal(r.log, undefined);
  assert.deepEqual([...fromBase64(r.body_b64)], [0, 255, 128, 10]);
  assert.deepEqual(Object.fromEntries(r.headers), { 'x-thing': 'a', 'content-type': 'image/png' });
});

test('GET sends no body, even when the request carries one', async () => {
  let seen;
  const fetchFn = async (url, init) => {
    seen = init;
    return response(200, new Uint8Array(), {});
  };
  const r = await perform(req({ method: 'GET', body_b64: toBase64(new Uint8Array([1])) }), fetchFn);
  assert.equal(seen.body, undefined);
  assert.equal(r.status, 200);
  assert.equal(r.body_b64, '');
});

test('a TypeError (CORS or network) is no_network with the exact log line', async () => {
  const fetchFn = async () => {
    throw new TypeError('Failed to fetch');
  };
  const r = await perform(req({ method: 'GET', url: 'https://api.example.com/a?b=1' }), fetchFn);
  assert.equal(r.id, 7);
  assert.equal(r.error, 'no_network');
  assert.equal(
    r.log,
    'net: GET https://api.example.com/a?b=1 failed in the browser — the server does not allow requests from web pages ' +
      '(CORS) or is unreachable; record a fixture with `inkemu run --net record` and commit it under ' +
      'tests/fixtures/<app_id>/net/',
  );
  assert.equal(r.log, corsLine('GET', 'https://api.example.com/a?b=1'));
});

test('no answer within timeout_ms is timeout', async () => {
  const fetchFn = (url, init) =>
    new Promise((_, reject) => {
      init.signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
    });
  const t0 = Date.now();
  const r = await perform(req({ timeout_ms: 50 }), fetchFn);
  assert.deepEqual(r, { id: 7, error: 'timeout' });
  assert.ok(Date.now() - t0 < 2000);
});

test('a body that stalls past timeout_ms is timeout too', async () => {
  const fetchFn = async (url, init) => ({
    status: 200,
    headers: new Headers(),
    arrayBuffer: () =>
      new Promise((_, reject) => {
        init.signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')));
      }),
  });
  const r = await perform(req({ timeout_ms: 50 }), fetchFn);
  assert.deepEqual(r, { id: 7, error: 'timeout' });
});

test('any other failure still answers the request', async () => {
  const fetchFn = async () => {
    throw new Error('boom');
  };
  const r = await perform(req(), fetchFn);
  assert.equal(r.id, 7);
  assert.equal(r.error, 'no_network');
  assert.match(r.log, /^net: POST https:\/\/api\.example\.com\/x failed: boom$/);
});

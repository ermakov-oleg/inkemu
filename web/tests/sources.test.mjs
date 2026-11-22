import { test } from 'node:test';
import assert from 'node:assert/strict';
import { zipSync } from '../vendor/fflate.mjs';
import {
  findAppRoot, findFixtures, fromZip, fromFileList, readAppId, TooLarge, TooManyFiles, NotAnApp,
} from '../js/sources.js';

const enc = new TextEncoder();
const manifest = enc.encode('{"app_id":"demo"}');

test('findAppRoot picks the shallowest manifest folder', () => {
  assert.equal(findAppRoot(['a/b/manifest.json', 'a/manifest.json', 'x']), 'a');
  assert.equal(findAppRoot(['manifest.json', 'a/manifest.json']), '');
  assert.equal(findAppRoot(['a/readme']), null);
  assert.equal(findAppRoot(['__MACOSX/manifest.json']), null);
});

test('findFixtures walks up to the root', () => {
  const paths = ['tests/fixtures/demo/net/a.json', 'apps/x/manifest.json', 'apps/x/y/z.lua'];
  assert.equal(findFixtures(paths, 'apps/x', 'demo'), 'tests/fixtures/demo');
  assert.equal(findFixtures(paths, 'apps/x', 'other'), null);
  assert.equal(findFixtures(['apps/tests/fixtures/demo/reader.json'], 'apps/x', 'demo'), 'apps/tests/fixtures/demo');
});

test('readAppId', () => {
  assert.equal(readAppId(manifest), 'demo');
  assert.equal(readAppId(enc.encode('nope')), null);
  assert.equal(readAppId(enc.encode('{}')), null);
});

test('fromZip with a wrapping folder, fixtures and byte-exact binary', async () => {
  const bin = new Uint8Array([0, 255, 1, 0, 255]);
  const zip = zipSync({
    'repo-main/apps/demo/manifest.json': manifest,
    'repo-main/apps/demo/index.lua': enc.encode('print(1)'),
    'repo-main/apps/demo/data/x.bin': bin,
    'repo-main/tests/fixtures/demo/net/GET_a.json': enc.encode('{}'),
    'repo-main/tests/fixtures/demo/reader.json': enc.encode('{}'),
    'repo-main/tests/fixtures/demo/other.txt': enc.encode('x'),
    'repo-main/tests/fixtures/zzz/net/a.json': enc.encode('{}'),
  });
  const r = await fromZip(zip.buffer.slice(zip.byteOffset, zip.byteOffset + zip.byteLength), 'a.zip');
  assert.equal(r.appId, 'demo');
  assert.equal(r.label, 'zip: a.zip');
  assert.deepEqual([...r.files.keys()].sort(), ['data/x.bin', 'index.lua', 'manifest.json']);
  assert.deepEqual([...r.files.get('data/x.bin')], [...bin]);
  assert.deepEqual([...r.fixtures.keys()].sort(), ['net/GET_a.json', 'reader.json']);
});

test('fromZip without a manifest is NotAnApp', async () => {
  const zip = zipSync({ 'a.txt': enc.encode('x') });
  await assert.rejects(fromZip(zip), NotAnApp);
});

test('fromZip limits', async () => {
  const many = { 'manifest.json': manifest };
  for (let i = 0; i < 2000; i++) many[`f${i}.txt`] = enc.encode('x');
  await assert.rejects(fromZip(zipSync(many)), (e) => e instanceof TooManyFiles && e.count === 2001 && /2000/.test(e.message));
  const big = zipSync({ 'manifest.json': manifest, 'big.bin': new Uint8Array(21 * 1024 * 1024) });
  await assert.rejects(fromZip(big), (e) => e instanceof TooLarge && /20 MiB/.test(e.message));
});

test('fromFileList uses webkitRelativePath', async () => {
  const f = (path, bytes) => ({ webkitRelativePath: path, size: bytes.length, arrayBuffer: async () => bytes.buffer.slice(0) });
  const r = await fromFileList([
    f('myapp/manifest.json', manifest),
    f('myapp/index.lua', enc.encode('x')),
    f('myapp/.git/config', enc.encode('x')),
  ]);
  assert.equal(r.label, 'folder: myapp');
  assert.deepEqual([...r.files.keys()].sort(), ['index.lua', 'manifest.json']);
});

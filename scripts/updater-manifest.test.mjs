// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';
import test from 'node:test';
import { buildManifest } from './updater-manifest.mjs';

const entries = ['QoreDB_aarch64.app.tar.gz', 'QoreDB_x64.app.tar.gz', 'QoreDB_0.1.40_amd64.AppImage', 'QoreDB_0.1.40_x64_en-US.msi']
  .map(name => ({ name, signature: 'signed-content' }));
const options = { version: '0.1.40', tag: 'v0.1.40', repo: 'QoreDB/QoreDB', entries };

test('updater manifest preserves default targets and bundle-specific aliases', () => {
  const result = buildManifest(options);
  assert.equal(Object.keys(result.platforms).length, 8);
  assert.equal(result.platforms['windows-x86_64'].url, 'https://github.com/QoreDB/QoreDB/releases/download/v0.1.40/QoreDB_0.1.40_x64_en-US.msi');
  assert.deepEqual(result.platforms['linux-x86_64'], result.platforms['linux-x86_64-appimage']);
});

test('refuses missing platforms, duplicate targets, absent signatures and mismatched tags', () => {
  assert.throws(() => buildManifest({ ...options, entries: entries.slice(1) }), /Incomplete updater matrix/);
  assert.throws(() => buildManifest({ ...options, entries: [...entries, entries[0]] }), /Duplicate updater target/);
  assert.throws(() => buildManifest({ ...options, entries: entries.map(e => ({ ...e, signature: '' })) }), /Missing signature/);
  assert.throws(() => buildManifest({ ...options, tag: 'v0.1.39' }), /disagree/);
});

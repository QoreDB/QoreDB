// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { fileURLToPath } from 'node:url';
import { gzipSync } from 'node:zlib';
import { compareReports, measureBundle } from './measure-bundle.mjs';

function fixture(t, manifest, files = {}) {
  const root = mkdtempSync(join(tmpdir(), 'qoredb-bundle-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const [name, content] of Object.entries({
    '.vite/manifest.json': JSON.stringify(manifest),
    ...files,
  })) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), content);
  }
  return root;
}

test('deduplicates shared chunks, cycles and CSS across entries, excluding dynamic imports from initial', t => {
  const root = fixture(
    t,
    {
      a: {
        file: 'a.js',
        isEntry: true,
        imports: ['shared'],
        dynamicImports: ['dynamic'],
        css: ['shared.css'],
      },
      b: { file: 'b.js', isEntry: true, imports: ['shared'] },
      shared: { file: 'shared.js', imports: ['a'], css: ['shared.css'] },
      dynamic: {
        file: 'dynamic.js',
        imports: ['shared'],
        css: ['dynamic.css'],
        assets: ['logo.svg'],
      },
    },
    {
      'a.js': 'aaa',
      'b.js': 'bb',
      'shared.js': 'shared',
      'dynamic.js': 'dynamic',
      'shared.css': 'css',
      'dynamic.css': 'dynamic-css',
      'logo.svg': 'svg',
      'public.txt': 'public',
      'a.js.map': '{}',
    }
  );
  const report = measureBundle(root);
  assert.deepEqual(report.entries, ['a', 'b']);
  assert.deepEqual(report.initial.js, {
    bytes: 11,
    gzip: ['aaa', 'bb', 'shared'].reduce(
      (size, file) => size + gzipSync(file, { level: 9 }).length,
      0
    ),
  });
  assert.equal(report.total.js.bytes, 18);
  assert.equal(report.initial.css.bytes, 3);
  assert.equal(report.total.css.bytes, 14);
  assert.equal(report.other.bytes, 9);
  assert.equal(report.maps.bytes, 2);
  assert.deepEqual(report.unreferenced, ['a.js.map', 'public.txt']);
  assert.equal(report.files.find(file => file.path === 'dynamic.js').initial, false);
});

test('counts emitted JS and CSS absent from manifest in total', t => {
  const root = fixture(
    t,
    { entry: { file: 'app.js', isEntry: true } },
    { 'app.js': 'app', 'worker.js': 'worker', 'extra.css': 'css' }
  );
  const report = measureBundle(root);
  assert.equal(report.initial.js.bytes, 3);
  assert.equal(report.total.js.bytes, 9);
  assert.equal(report.total.css.bytes, 3);
  assert.deepEqual(report.unreferenced, ['extra.css', 'worker.js']);
});

for (const [name, manifest, expected] of [
  ['missing entry', { app: { file: 'app.js' } }, /no entry/],
  ['missing output', { app: { file: 'missing.js', isEntry: true } }, /output file/],
  [
    'missing static import',
    { app: { file: 'app.js', isEntry: true, imports: ['missing'] } },
    /manifest import/,
  ],
  [
    'missing dynamic import',
    { app: { file: 'app.js', isEntry: true, dynamicImports: ['missing'] } },
    /manifest import/,
  ],
  ['missing CSS', { app: { file: 'app.js', isEntry: true, css: ['missing.css'] } }, /output file/],
  ['invalid list', { app: { file: 'app.js', isEntry: true, imports: 'app' } }, /Invalid imports/],
  ['path traversal', { app: { file: '../app.js', isEntry: true } }, /output file/],
  ['invalid manifest', [], /Invalid Vite manifest/],
  ['invalid entry flag', { app: { file: 'app.js', isEntry: 'true' } }, /Invalid isEntry/],
]) {
  test(`rejects ${name}`, t => {
    assert.throws(() => measureBundle(fixture(t, manifest, { 'app.js': 'app' })), expected);
  });
}

test('rejects malformed JSON', t => {
  assert.throws(() => measureBundle(fixture(t, {}, { '.vite/manifest.json': '{' })), SyntaxError);
});

function report() {
  return {
    schemaVersion: 1,
    metadata: {
      commit: 'a',
      dirty: false,
      compatibility: {
        node: 'v26',
        pnpm: '11',
        vite: '8',
        zlib: '1',
        platform: 'linux',
        arch: 'x64',
        osRelease: '1',
        mode: 'production',
        configSha256: 'config',
        lockfileSha256: 'lock',
        envSha256: 'env',
        gzipLevel: 9,
        buildArgs: ['build'],
        envFiles: {},
      },
    },
    initial: { js: { bytes: 100, gzip: 50 }, css: { bytes: 0, gzip: 0 } },
    total: { js: { bytes: 1000, gzip: 500 }, css: { bytes: 0, gzip: 0 } },
  };
}

test('accepts different source commits but only comparable build metadata', () => {
  const current = report();
  current.metadata.commit = 'b';
  current.metadata.dirty = true;
  assert.equal(compareReports(current, report()).status, 'passed');
  current.metadata.compatibility.node = 'v27';
  assert.deepEqual(compareReports(current, report()), {
    status: 'incomparable',
    differences: ['node'],
  });
});

test('enforces each initial metric and the inclusive two percent total boundary', () => {
  const current = report();
  current.total.js.bytes = 1020;
  current.total.js.gzip = 510;
  assert.equal(compareReports(current, report()).status, 'passed');
  current.total.js.bytes += 1;
  assert.equal(compareReports(current, report()).status, 'failed');
  current.total.js.bytes = 1000;
  current.initial.js.gzip += 1;
  assert.equal(compareReports(current, report()).status, 'failed');
});

test('zero baseline permits zero and rejects new cost without an infinite JSON percentage', () => {
  const current = report();
  current.total.css.bytes = 1;
  const comparison = compareReports(current, report());
  assert.equal(comparison.status, 'failed');
  assert.equal(
    comparison.checks.find(check => check.metric === 'total.css.bytes').growthPercent,
    null
  );
});

test('rejects invalid, unsupported or incomplete baseline reports', () => {
  for (const baseline of [null, {}, { ...report(), schemaVersion: 2 }]) {
    assert.throws(() => compareReports(report(), baseline), /report schema/);
  }
  const baseline = report();
  baseline.total.js.bytes = -1;
  assert.throws(() => compareReports(report(), baseline), /Invalid size/);
  baseline.total.js.bytes = 1;
  assert.throws(() => compareReports(report(), baseline), /Total below initial/);
  delete baseline.metadata.compatibility.node;
  assert.throws(() => compareReports(report(), baseline), /Invalid metadata/);
});

test('CLI rejects explicitly supplied falsy baselines before starting a build', t => {
  const root = fixture(t, {});
  const baseline = join(root, 'baseline.json');
  for (const value of [null, false, 0]) {
    writeFileSync(baseline, JSON.stringify(value));
    const result = spawnSync(
      process.execPath,
      [fileURLToPath(new URL('./measure-bundle.mjs', import.meta.url)), '--baseline', baseline],
      { encoding: 'utf8' }
    );
    assert.equal(result.status, 1);
    assert.match(result.stderr, /Unsupported report schema/);
    assert.equal(result.stdout, '');
  }
});

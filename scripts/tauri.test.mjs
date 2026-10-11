// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, mkdirSync, copyFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

test('Linux packaging avoids linuxdeploy stripping modern ELF libraries', { skip: process.platform !== 'linux' }, t => {
  const root = mkdtempSync(join(tmpdir(), 'qoredb-tauri-wrapper-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  copyFileSync('scripts/tauri.mjs', join(root, 'tauri.mjs'));
  const cli = join(root, 'node_modules/@tauri-apps/cli');
  mkdirSync(cli, { recursive: true });
  writeFileSync(join(cli, 'tauri.js'), 'console.log(JSON.stringify({args:process.argv.slice(2),noStrip:process.env.NO_STRIP}));');
  const env = { ...process.env };
  delete env.NO_STRIP;
  for (const command of ['build', 'bundle', 'dev']) {
    const result = spawnSync(process.execPath, [join(root, 'tauri.mjs'), command], { env, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    const received = JSON.parse(result.stdout);
    assert.equal(received.noStrip, command === 'dev' ? undefined : '1', command);
    if (command === 'build') assert.ok(received.args.includes('duckdb-bundled'));
  }
});

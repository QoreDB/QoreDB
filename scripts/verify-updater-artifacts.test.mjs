// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import test from 'node:test';

const minisign = process.env.MINISIGN_BIN ?? 'minisign';
const available = spawnSync(minisign, ['-v']).status === 0;

test('verifies real signatures and rejects changed bytes or a different trust root', { skip: !available }, t => {
  const root = mkdtempSync(join(tmpdir(), 'qore-updater-test-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const directory of ['scripts', 'src-tauri', 'artifacts']) mkdirSync(join(root, directory));
  for (const script of ['verify-updater-artifacts.mjs', 'updater-manifest.mjs']) {
    copyFileSync(new URL(script, import.meta.url), join(root, 'scripts', script));
  }
  writeFileSync(join(root, 'package.json'), JSON.stringify({ version: '0.1.40' }));
  const run = args => {
    const result = spawnSync(minisign, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  };
  const pub = join(root, 'test.pub');
  const key = join(root, 'test.key');
  run(['-G', '-W', '-p', pub, '-s', key]);
  const config = () => writeFileSync(join(root, 'src-tauri/tauri.conf.json'), JSON.stringify({
    plugins: { updater: { pubkey: readFileSync(pub).toString('base64') } },
  }));
  config();
  const names = ['QoreDB_aarch64.app.tar.gz', 'QoreDB_x64.app.tar.gz', 'QoreDB_0.1.40_amd64.AppImage', 'QoreDB_0.1.40_x64.msi'];
  for (const name of names) {
    const file = join(root, 'artifacts', name);
    writeFileSync(file, 'synthetic signed artifact');
    run(['-S', '-W', '-s', key, '-m', file]);
    writeFileSync(file + '.sig', readFileSync(file + '.minisig').toString('base64'));
  }
  const verify = () => spawnSync(process.execPath, [join(root, 'scripts/verify-updater-artifacts.mjs'), join(root, 'artifacts'), join(root, 'latest.json')], { encoding: 'utf8' });
  assert.equal(verify().status, 0);
  const changed = join(root, 'artifacts', names[0]);
  writeFileSync(changed, 'tampered artifact');
  assert.match(verify().stderr, /Invalid updater signature/);
  writeFileSync(changed, 'synthetic signed artifact');
  run(['-G', '-W', '-f', '-p', pub, '-s', key]);
  config();
  assert.match(verify().stderr, /Invalid updater signature/);
});

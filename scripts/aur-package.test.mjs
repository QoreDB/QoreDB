// SPDX-License-Identifier: Apache-2.0
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import test from 'node:test';

test('AUR installs sidecars and application resources from the Debian payload', t => {
  const root = mkdtempSync(join(tmpdir(), 'qoredb-aur-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const payload = {
    'usr/bin/qoredb': 'desktop',
    'usr/bin/qore': 'cli',
    'usr/bin/qore-mcp': 'mcp',
    'usr/lib/qoredb/helper.so': 'library',
    'usr/share/qoredb/licenses/LICENSE': 'core licence',
    'usr/share/qoredb/licenses/LICENSE-BSL': 'premium licence',
    'usr/share/applications/QoreDB.desktop': '[Desktop Entry]',
    'usr/share/icons/hicolor/32x32/apps/qoredb.png': 'icon',
  };
  for (const [name, value] of Object.entries(payload)) {
    const file = join(root, 'src/deb', name);
    mkdirSync(dirname(file), { recursive: true });
    writeFileSync(file, value);
  }
  mkdirSync(join(root, 'pkg'));
  const result = spawnSync('bash', ['-eu', '-c', 'source "$1"; package', 'test', resolve('aur/qoredb-bin/PKGBUILD')], {
    env: { ...process.env, srcdir: join(root, 'src'), pkgdir: join(root, 'pkg') }, encoding: 'utf8',
  });
  assert.equal(result.status, 0, result.stderr);
  for (const [name, value] of Object.entries(payload)) {
    const installed = name === 'usr/share/applications/QoreDB.desktop' ? 'usr/share/applications/qoredb.desktop' : name;
    assert.equal(readFileSync(join(root, 'pkg', installed), 'utf8'), value, installed);
  }
});

test('AUR updater pins the artifact checksum and rejects invalid versions before editing', t => {
  const root = mkdtempSync(join(tmpdir(), 'qoredb-aur-update-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  for (const name of ['scripts/update-aur.sh', 'aur/qoredb-bin/PKGBUILD', 'aur/qoredb-bin/.SRCINFO']) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), readFileSync(name));
  }
  const artifact = join(root, 'fixture.deb');
  writeFileSync(artifact, 'synthetic artifact bytes');
  const script = join(root, 'scripts/update-aur.sh');
  const update = spawnSync('bash', [script, '0.1.39', artifact], { encoding: 'utf8' });
  assert.equal(update.status, 0, update.stderr);
  const hash = createHash('sha256').update(readFileSync(artifact)).digest('hex');
  const files = ['PKGBUILD', '.SRCINFO'].map(name => join(root, 'aur/qoredb-bin', name));
  const before = files.map(name => readFileSync(name, 'utf8'));
  for (const content of before) {
    assert.ok(content.includes(hash));
    assert.ok(!content.includes('SKIP'));
  }
  assert.notEqual(spawnSync('bash', [script, 'feat/v0-1-40', artifact]).status, 0);
  assert.notEqual(spawnSync('bash', [script, '0.1.40', join(root, 'missing.deb')]).status, 0);
  assert.deepEqual(files.map(name => readFileSync(name, 'utf8')), before);
});

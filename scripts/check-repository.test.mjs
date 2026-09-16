// SPDX-License-Identifier: Apache-2.0

import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join } from 'node:path';
import test from 'node:test';
import { checkRepository, localLinks } from './check-repository.mjs';

function fixture(t, overrides = {}) {
  const root = mkdtempSync(join(tmpdir(), 'qoredb-repo-check-'));
  t.after(() => rmSync(root, { recursive: true, force: true }));
  const files = {
    'AGENTS.md': '# Shared instructions\n',
    'CLAUDE.md': '@AGENTS.md\n',
    'aur/qoredb-bin/.SRCINFO': 'pkgbase = qoredb-bin\n',
    'aur/qoredb-bin/PKGBUILD': 'pkgname=qoredb-bin\n',
    'doc/README.md': '[Audits](audits/README.md)\n[Guide](development/guide.md)\n',
    'doc/audits/README.md': '# Audits\n',
    'doc/development/guide.md': '[Source](../../src/example.ts)\n',
    'src/example.ts': '// SPDX-License-Identifier: Apache-2.0\nexport {};\n',
    ...overrides,
  };
  for (const [name, content] of Object.entries(files)) {
    mkdirSync(dirname(join(root, name)), { recursive: true });
    writeFileSync(join(root, name), content);
  }
  return { root, files: Object.keys(files) };
}

function errors(t, overrides) {
  const repo = fixture(t, overrides);
  return checkRepository(repo.root, repo.files).errors;
}

test('resolves inline and reference links while ignoring examples and external URLs', () => {
  const markdown = '[File](../src/a.ts#symbol) [Site](https://example.com) [Here](#heading)\n' +
    '[space](<../some%20file.md>)\n[ref]: ../src/b.ts "Title"\n' +
    '`[example](missing.md)`\n```md\n[example](also-missing.md)\n```\n';
  assert.deepEqual(localLinks(markdown, 'doc/README.md'), ['src/a.ts', 'some file.md', 'src/b.ts']);
});

test('accepts a clean checkout with linked docs and both supported licenses', t => {
  assert.deepEqual(errors(t, { 'src/premium.ts': '// SPDX-License-Identifier: BUSL-1.1\n' }), []);
});

test('rejects broken links and links that depend on ignored local material', t => {
  const repo = fixture(t, { 'doc/development/guide.md': '[Private](../private/local.md)\n[Missing](missing.md)\n' });
  mkdirSync(join(repo.root, 'doc/private'));
  writeFileSync(join(repo.root, 'doc/private/local.md'), 'Exists locally but not in Git.');
  const result = checkRepository(repo.root, repo.files);
  assert.equal(result.errors.length, 2);
  assert.ok(result.errors.every(error => error.includes('unavailable local link')));
});

test('requires discoverability for new public docs but excludes historical and private notes', t => {
  const result = errors(t, {
    'doc/development/new.md': '# New guide\n',
    'doc/archive/old.md': '# Historical\n',
    'doc/private/local.md': '# Local\n',
  });
  assert.equal(result.length, 1);
  assert.match(result[0], /doc\/development\/new.md: add a link/);
});

test('rejects duplicated Claude instructions and oversized shared instructions', t => {
  const result = errors(t, { 'CLAUDE.md': '@AGENTS.md\nMore rules\n', 'AGENTS.md': 'x'.repeat(8193) });
  assert.equal(result.length, 2);
  assert.ok(result.some(error => error.includes('must only import')));
  assert.ok(result.some(error => error.includes('instruction budget')));
});

test('rejects missing scoped imports and invalid SPDX identifiers without relicensing vendor code', t => {
  const result = errors(t, {
    'src/AGENTS.md': '# Frontend\n',
    'src/example.ts': '// SPDX-License-Identifier: BSL-1.1\n',
    'src-tauri/vendor/upstream.rs': '// Upstream license\n',
  });
  assert.equal(result.length, 2);
  assert.ok(result.some(error => error.includes('missing Claude import')));
  assert.ok(result.some(error => error.includes('first-line SPDX')));
});

test('does not require deleted working-tree files to exist', t => {
  const repo = fixture(t);
  assert.deepEqual(checkRepository(repo.root, [...repo.files, 'deleted.ts']).errors, []);
});

test('requires the AUR package files used by the release workflow', t => {
  const repo = fixture(t);
  const files = repo.files.filter(file => !file.startsWith('aur/qoredb-bin/'));
  assert.deepEqual(checkRepository(repo.root, files).errors, [
    'Missing entry point: aur/qoredb-bin/.SRCINFO',
    'Missing entry point: aur/qoredb-bin/PKGBUILD',
  ]);
});

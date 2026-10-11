// SPDX-License-Identifier: Apache-2.0

import { spawnSync } from 'node:child_process';
import { mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, join, resolve } from 'node:path';
import { buildManifest, platformKeys } from './updater-manifest.mjs';

const [directory, output] = process.argv.slice(2);
if (!directory || !output) throw new Error('Usage: verify-updater-artifacts.mjs DIRECTORY OUTPUT');
const config = JSON.parse(readFileSync(new URL('../src-tauri/tauri.conf.json', import.meta.url), 'utf8'));
const version = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
const scratch = mkdtempSync(join(tmpdir(), 'qore-signature-'));

function walk(path) {
  return readdirSync(path, { withFileTypes: true }).flatMap(entry => {
    const child = join(path, entry.name);
    return entry.isDirectory() ? walk(child) : entry.isFile() ? [child] : [];
  });
}

try {
  const pubkey = join(scratch, 'updater.pub');
  writeFileSync(pubkey, Buffer.from(config.plugins.updater.pubkey, 'base64'));
  const entries = [];
  for (const file of walk(resolve(directory))) {
    if (!file.endsWith('.sig')) continue;
    const bundle = file.slice(0, -4);
    const name = basename(bundle);
    if (!platformKeys(name)) continue;
    const signature = readFileSync(file, 'utf8').trim();
    const decoded = join(scratch, 'artifact.minisig');
    writeFileSync(decoded, Buffer.from(signature, 'base64'));
    const check = spawnSync(process.env.MINISIGN_BIN ?? 'minisign', [
      '-V', '-q', '-m', bundle, '-p', pubkey, '-x', decoded,
    ], { encoding: 'utf8' });
    if (check.error || check.status !== 0) {
      throw new Error(`Invalid updater signature for ${name}: ${check.error?.message ?? check.stderr}`);
    }
    console.log(`Verified ${name}`);
    entries.push({ name, signature });
  }
  const manifest = buildManifest({ version, repo: 'QoreDB/QoreDB', tag: `v${version}`, entries });
  writeFileSync(output, JSON.stringify(manifest, null, 2) + '\n');
  console.log(`Verified complete updater matrix for ${version}; manifest saved locally`);
} finally {
  rmSync(scratch, { recursive: true, force: true });
}

import { execFileSync } from 'node:child_process';
import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const pkgPath = path.join(root, 'package.json');
const lockPath = path.join(root, 'src-tauri', 'Cargo.lock');

const pkgRaw = readFileSync(pkgPath, 'utf8');
const current = JSON.parse(pkgRaw).version;
const parts = /^(\d+)\.(\d+)\.(\d+)$/.exec(current ?? '');

if (!parts) {
  console.error(`Unsupported package.json version: ${current}`);
  process.exit(1);
}

const next = `${parts[1]}.${parts[2]}.${Number(parts[3]) + 1}`;

writeFileSync(pkgPath, pkgRaw.replace(`"version": "${current}"`, `"version": "${next}"`));

const lock = readFileSync(lockPath, 'utf8');
const lockPattern = new RegExp(
  `(name = "qoredb"\\r?\\nversion = )"${current.replace(/\./g, '\\.')}"`
);

if (!lockPattern.test(lock)) {
  console.error(`Could not find qoredb ${current} in Cargo.lock.`);
  process.exit(1);
}

writeFileSync(lockPath, lock.replace(lockPattern, `$1"${next}"`));

execFileSync('node', [path.join('scripts', 'sync-version.mjs')], { stdio: 'inherit' });

console.log(`Bumped version ${current} → ${next}`);

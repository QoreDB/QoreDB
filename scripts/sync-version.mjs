import { readFileSync, writeFileSync } from 'node:fs';
import path from 'node:path';

const root = process.cwd();
const pkgPath = path.join(root, 'package.json');
const cargoPath = path.join(root, 'src-tauri', 'Cargo.toml');

const pkg = JSON.parse(readFileSync(pkgPath, 'utf8'));
const version = pkg?.version;

if (!version) {
  console.error('package.json version is missing.');
  process.exit(1);
}

const cargo = readFileSync(cargoPath, 'utf8');
const lines = cargo.split(/\r?\n/);

let inPackage = false;
let updated = false;

for (let i = 0; i < lines.length; i += 1) {
  const line = lines[i];

  if (/^\s*\[package\]\s*$/.test(line)) {
    inPackage = true;
    continue;
  }

  if (inPackage && /^\s*\[.+\]\s*$/.test(line)) {
    inPackage = false;
  }

  if (inPackage && /^\s*version\s*=/.test(line)) {
    lines[i] = `version = "${version}"`;
    updated = true;
    break;
  }
}

if (!updated) {
  console.error('Could not find package.version in Cargo.toml.');
  process.exit(1);
}

writeFileSync(cargoPath, lines.join('\n'));
console.log(`Synced Cargo.toml version to ${version}`);

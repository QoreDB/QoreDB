// SPDX-License-Identifier: Apache-2.0

import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { mkdirSync, readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { arch, platform, release } from 'node:os';
import { dirname, resolve, sep } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';
import { gzipSync } from 'node:zlib';

const gzipLevel = 9;
const schemaVersion = 1;
const buildArgs = [
  'exec',
  'vite',
  'build',
  '--mode',
  'production',
  '--manifest',
  '--outDir',
  '.perf/dist',
  '--emptyOutDir',
];
const hash = bytes => createHash('sha256').update(bytes).digest('hex');
const record = value => value && typeof value === 'object' && !Array.isArray(value);
const requireValid = (condition, message) => {
  if (!condition) throw new Error(message);
};

function inventory(root, prefix = '') {
  return readdirSync(resolve(root, prefix), { withFileTypes: true }).flatMap(entry => {
    const name = `${prefix}${entry.name}`;
    if (entry.isDirectory()) return inventory(root, `${name}/`);
    requireValid(entry.isFile(), `Unsupported output entry: ${name}`);
    return [name];
  });
}

export function measureBundle(root) {
  const manifestPath = '.vite/manifest.json';
  const manifest = JSON.parse(readFileSync(resolve(root, manifestPath), 'utf8'));
  requireValid(record(manifest), 'Invalid Vite manifest');
  const paths = inventory(root)
    .filter(name => name !== manifestPath)
    .sort();
  const available = new Set(paths);
  const referenced = new Set();
  const checkFile = name => {
    requireValid(
      typeof name === 'string' && available.has(name),
      `Missing or invalid output file: ${name}`
    );
    referenced.add(name);
  };
  const checkList = (entry, field, key) => {
    requireValid(
      entry[field] === undefined || Array.isArray(entry[field]),
      `Invalid ${field}: ${key}`
    );
    return entry[field] ?? [];
  };
  for (const [key, entry] of Object.entries(manifest)) {
    requireValid(record(entry), `Invalid manifest entry: ${key}`);
    requireValid(
      entry.isEntry === undefined || typeof entry.isEntry === 'boolean',
      `Invalid isEntry: ${key}`
    );
    checkFile(entry.file);
    for (const field of ['css', 'assets'])
      for (const name of checkList(entry, field, key)) checkFile(name);
    for (const field of ['imports', 'dynamicImports']) {
      for (const target of checkList(entry, field, key)) {
        requireValid(
          typeof target === 'string' && Object.hasOwn(manifest, target),
          `Missing manifest import: ${target}`
        );
      }
    }
  }
  const entries = Object.keys(manifest)
    .filter(key => manifest[key].isEntry)
    .sort();
  requireValid(entries.length > 0, 'Manifest has no entry point');
  const visited = new Set();
  const initial = new Set();
  function visit(key) {
    if (visited.has(key)) return;
    visited.add(key);
    const entry = manifest[key];
    initial.add(entry.file);
    for (const css of entry.css ?? []) initial.add(css);
    for (const dependency of entry.imports ?? []) visit(dependency);
  }
  for (const key of entries) visit(key);
  const files = paths.map(path => {
    const bytes = readFileSync(resolve(root, path));
    const kind = path.endsWith('.map')
      ? 'map'
      : /\.(?:m?js)$/.test(path)
        ? 'js'
        : path.endsWith('.css')
          ? 'css'
          : 'other';
    return {
      path,
      kind,
      initial: initial.has(path),
      manifestReferenced: referenced.has(path),
      bytes: bytes.length,
      gzip: gzipSync(bytes, { level: gzipLevel }).length,
    };
  });
  const sum = selection =>
    selection.reduce(
      (total, file) => ({ bytes: total.bytes + file.bytes, gzip: total.gzip + file.gzip }),
      { bytes: 0, gzip: 0 }
    );
  const sizes = scope =>
    Object.fromEntries(
      ['js', 'css'].map(kind => [
        kind,
        sum(files.filter(file => file.kind === kind && (scope === 'total' || file.initial))),
      ])
    );
  return {
    entries,
    files,
    initial: sizes('initial'),
    total: sizes('total'),
    other: sum(files.filter(file => file.kind === 'other')),
    maps: sum(files.filter(file => file.kind === 'map')),
    unreferenced: files.filter(file => !file.manifestReferenced).map(file => file.path),
  };
}

export function metadata(root) {
  const run = (command, args) =>
    execFileSync(command, args, { cwd: root, encoding: 'utf8' }).trim();
  const digest = name => hash(readFileSync(resolve(root, name)));
  // Store only a digest of local configuration, never its potentially secret contents.
  const envFiles = ['.env', '.env.local', '.env.production', '.env.production.local'];
  const environment = Object.fromEntries(
    Object.entries(process.env)
      .filter(([key]) => /^(VITE_|TAURI_|NODE_ENV$|BROWSERSLIST|SOURCE_DATE_EPOCH$)/.test(key))
      .sort()
  );
  return {
    commit: run('git', ['rev-parse', 'HEAD']),
    dirty: run('git', ['status', '--porcelain']).length > 0,
    compatibility: {
      node: process.version,
      pnpm: run('pnpm', ['--version']),
      vite: run('pnpm', ['exec', 'vite', '--version']),
      zlib: process.versions.zlib,
      platform: platform(),
      arch: arch(),
      osRelease: release(),
      mode: 'production',
      gzipLevel,
      buildArgs,
      configSha256: digest('vite.config.ts'),
      lockfileSha256: digest('pnpm-lock.yaml'),
      envSha256: hash(JSON.stringify(environment)),
      envFiles: Object.fromEntries(
        envFiles.map(name => {
          try {
            return [name, digest(name)];
          } catch (error) {
            if (error.code === 'ENOENT') return [name, null];
            throw error;
          }
        })
      ),
    },
  };
}

function validateReport(report) {
  requireValid(
    record(report) && report.schemaVersion === schemaVersion,
    'Unsupported report schema'
  );
  requireValid(record(report.metadata?.compatibility), 'Missing compatibility metadata');
  for (const field of [
    'node',
    'pnpm',
    'vite',
    'zlib',
    'platform',
    'arch',
    'osRelease',
    'mode',
    'configSha256',
    'lockfileSha256',
    'envSha256',
  ]) {
    requireValid(
      typeof report.metadata.compatibility[field] === 'string',
      `Invalid metadata: ${field}`
    );
  }
  requireValid(report.metadata.compatibility.gzipLevel === gzipLevel, 'Incompatible gzip level');
  requireValid(
    Array.isArray(report.metadata.compatibility.buildArgs) &&
      record(report.metadata.compatibility.envFiles),
    'Missing build metadata'
  );
  for (const scope of ['initial', 'total'])
    for (const kind of ['js', 'css'])
      for (const metric of ['bytes', 'gzip']) {
        const value = report[scope]?.[kind]?.[metric];
        requireValid(
          Number.isSafeInteger(value) && value >= 0,
          `Invalid size: ${scope}.${kind}.${metric}`
        );
        if (scope === 'total')
          requireValid(
            value >= report.initial[kind][metric],
            `Total below initial: ${kind}.${metric}`
          );
      }
}

export function compareReports(current, baseline) {
  validateReport(current);
  validateReport(baseline);
  const differences = [
    ...new Set([
      ...Object.keys(current.metadata.compatibility),
      ...Object.keys(baseline.metadata.compatibility),
    ]),
  ].filter(
    key =>
      JSON.stringify(current.metadata.compatibility[key]) !==
      JSON.stringify(baseline.metadata.compatibility[key])
  );
  if (differences.length) return { status: 'incomparable', differences };
  const checks = [];
  for (const scope of ['initial', 'total'])
    for (const kind of ['js', 'css'])
      for (const metric of ['bytes', 'gzip']) {
        const before = baseline[scope][kind][metric];
        const after = current[scope][kind][metric];
        const maximumGrowthPercent = scope === 'initial' ? 0 : 2;
        checks.push({
          metric: `${scope}.${kind}.${metric}`,
          before,
          after,
          delta: after - before,
          growthPercent: before === 0 ? (after === 0 ? 0 : null) : (after / before - 1) * 100,
          maximumGrowthPercent,
          passed: after * 100 <= before * (100 + maximumGrowthPercent),
        });
      }
  return { status: checks.every(check => check.passed) ? 'passed' : 'failed', checks };
}

export function makeReport(root, outputDirectory) {
  return {
    schemaVersion,
    generatedAt: new Date().toISOString(),
    metadata: metadata(root),
    ...measureBundle(outputDirectory),
  };
}

function main() {
  const root = fileURLToPath(new URL('../', import.meta.url));
  const { values } = parseArgs({
    options: {
      output: { type: 'string', default: '.perf/current.json' },
      baseline: { type: 'string' },
    },
  });
  const output = resolve(root, values.output);
  const buildDirectory = resolve(root, '.perf/dist');
  const inBuildDirectory = path =>
    path === buildDirectory || path.startsWith(`${buildDirectory}${sep}`);
  requireValid(
    !values.baseline || resolve(root, values.baseline) !== output,
    'Output must not overwrite baseline'
  );
  requireValid(!inBuildDirectory(output), 'Report output must be outside the build directory');
  requireValid(
    !values.baseline || !inBuildDirectory(resolve(root, values.baseline)),
    'Baseline must be outside the build directory'
  );
  const baseline = values.baseline
    ? JSON.parse(readFileSync(resolve(root, values.baseline), 'utf8'))
    : null;
  if (values.baseline) validateReport(baseline);
  execFileSync('pnpm', buildArgs, { cwd: root, stdio: 'inherit' });
  const report = makeReport(root, resolve(root, '.perf/dist'));
  report.comparison = baseline ? compareReports(report, baseline) : { status: 'not-requested' };
  mkdirSync(dirname(output), { recursive: true });
  writeFileSync(output, `${JSON.stringify(report, null, 2)}\n`);
  console.log(
    JSON.stringify(
      { output, initial: report.initial, total: report.total, comparison: report.comparison },
      null,
      2
    )
  );
  if (report.comparison.status === 'failed' || report.comparison.status === 'incomparable')
    process.exitCode = 1;
}

if (process.argv[1] && import.meta.url === pathToFileURL(resolve(process.argv[1])).href) {
  try {
    main();
  } catch (error) {
    console.error(error.message);
    process.exitCode = 1;
  }
}

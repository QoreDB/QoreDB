// SPDX-License-Identifier: Apache-2.0

// Rebuilds latest.json from the .sig assets already attached to the release.
// tauri-action lets every matrix job read-modify-write that file, and the four
// of them race: a job can 404 on an asset another one just replaced, and
// platform entries get lost. One job, one write.

import { readFileSync } from 'node:fs';
import { buildManifest, platformKeys } from './updater-manifest.mjs';

const token = process.env.GITHUB_TOKEN;
const repo = process.env.REPO;
const tag = process.env.TAG;
const releaseId = process.env.RELEASE_ID;

for (const [name, value] of Object.entries({ token, repo, tag, releaseId })) {
  if (!value) {
    console.error(`Missing required environment variable: ${name}`);
    process.exit(1);
  }
}

const version = JSON.parse(readFileSync(new URL('../package.json', import.meta.url), 'utf8')).version;
if (tag !== `v${version}`) throw new Error('Release tag and package version disagree');
const api = 'https://api.github.com';

async function gh(url, { method = 'GET', accept = 'application/vnd.github+json', body } = {}) {
  const response = await fetch(url.startsWith('http') ? url : `${api}${url}`, {
    method,
    headers: {
      authorization: `Bearer ${token}`,
      accept,
      'x-github-api-version': '2022-11-28',
      ...(body ? { 'content-type': 'application/json' } : {}),
    },
    body,
  });

  if (!response.ok) {
    throw new Error(`${method} ${url} → ${response.status} ${await response.text()}`);
  }

  return response;
}

const release = await (await gh(`/repos/${repo}/releases/${releaseId}`)).json();
if (release.tag_name !== tag) throw new Error('Release ID does not match the requested tag');

const assets = [];
for (let page = 1; ; page += 1) {
  const batch = await (
    await gh(`/repos/${repo}/releases/${releaseId}/assets?per_page=100&page=${page}`)
  ).json();
  assets.push(...batch);
  if (batch.length < 100) break;
}

const entries = [];
for (const asset of assets) {
  if (!asset.name.endsWith('.sig')) continue;

  const bundleName = asset.name.slice(0, -'.sig'.length);
  const keys = platformKeys(bundleName);
  if (!keys) {
    console.log(`Skipping ${asset.name}: no updater platform for this bundle`);
    continue;
  }

  if (!assets.some(a => a.name === bundleName)) {
    throw new Error(`${asset.name} has no matching bundle ${bundleName} on the release`);
  }

  const signature = (
    await (
      await gh(`/repos/${repo}/releases/assets/${asset.id}`, {
        accept: 'application/octet-stream',
      })
    ).text()
  ).trim();

  entries.push({ name: bundleName, signature });
}

const manifest = buildManifest({ version, repo, tag, notes: release.body ?? '', entries });
const { platforms } = manifest;

console.log(`Platforms: ${Object.keys(platforms).sort().join(', ')}`);

const existing = assets.find(a => a.name === 'latest.json');
if (existing) {
  await gh(`/repos/${repo}/releases/assets/${existing.id}`, { method: 'DELETE' });
}

const upload = await fetch(
  `https://uploads.github.com/repos/${repo}/releases/${releaseId}/assets?name=latest.json`,
  {
    method: 'POST',
    headers: {
      authorization: `Bearer ${token}`,
      accept: 'application/vnd.github+json',
      'content-type': 'application/json',
    },
    body: JSON.stringify(manifest, null, 2),
  }
);

if (!upload.ok) {
  throw new Error(`Uploading latest.json → ${upload.status} ${await upload.text()}`);
}

console.log(`latest.json published on ${tag} for version ${version}`);

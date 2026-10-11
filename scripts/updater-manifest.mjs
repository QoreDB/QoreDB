// SPDX-License-Identifier: Apache-2.0

/** Platform keys an updater bundle answers to, or null if it is not updatable. */
export function platformKeys(bundleName) {
  const arch = /aarch64|arm64/i.test(bundleName) ? 'aarch64' : 'x86_64';

  if (bundleName.endsWith('.app.tar.gz')) return [`darwin-${arch}`, `darwin-${arch}-app`];
  if (bundleName.endsWith('.AppImage')) return [`linux-${arch}`, `linux-${arch}-appimage`];
  if (bundleName.endsWith('.deb')) return [`linux-${arch}-deb`];
  if (bundleName.endsWith('.rpm')) return [`linux-${arch}-rpm`];
  // The MSI is the default Windows target, matching what tauri-action produced
  // until now — switching it would silently change which installer users get.
  if (bundleName.endsWith('.msi')) return [`windows-${arch}`, `windows-${arch}-msi`];
  if (bundleName.endsWith('.exe')) return [`windows-${arch}-nsis`];

  return null;
}

export const requiredPlatforms = ['darwin-aarch64', 'darwin-x86_64', 'linux-x86_64', 'windows-x86_64'];

export function buildManifest({ version, repo, tag, notes = '', entries }) {
  if (tag !== `v${version}`) throw new Error('Release tag and package version disagree');
  const platforms = {};
  for (const { name, signature } of entries) {
    const keys = platformKeys(name);
    if (!keys) continue;
    if (!signature?.trim()) throw new Error(`Missing signature for ${name}`);
    for (const key of keys) {
      if (platforms[key]) throw new Error(`Duplicate updater target: ${key}`);
      platforms[key] = {
        signature: signature.trim(),
        url: `https://github.com/${repo}/releases/download/${tag}/${encodeURIComponent(name)}`,
      };
    }
  }
  const missing = requiredPlatforms.filter(key => !platforms[key]);
  if (missing.length) throw new Error(`Incomplete updater matrix: ${missing.join(', ')}`);
  return { version, notes, pub_date: new Date().toISOString(), platforms };
}

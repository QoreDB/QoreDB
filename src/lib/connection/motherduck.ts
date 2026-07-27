// SPDX-License-Identifier: Apache-2.0

const OFFICIAL_HOST_PATTERN = /^pg\.[a-z0-9-]+\.motherduck\.com$/i;

export function motherDuckHostFromToken(token: string): string | null {
  const payload = token.trim().split('.')[1];
  if (!payload) return null;

  try {
    const padded = payload
      .replace(/-/g, '+')
      .replace(/_/g, '/')
      .padEnd(Math.ceil(payload.length / 4) * 4, '=');
    const claims = JSON.parse(atob(padded)) as { mdRegion?: unknown };
    if (typeof claims.mdRegion !== 'string') return null;

    const match = /^([a-z0-9]+)-([a-z0-9]+(?:-[a-z0-9]+)*)$/i.exec(claims.mdRegion);
    if (!match || match[1].toLowerCase() !== 'aws') return null;

    return `pg.${match[2].toLowerCase()}-aws.motherduck.com`;
  } catch {
    return null;
  }
}

export function resolveMotherDuckHost(host: string, token: string): string {
  const tokenHost = motherDuckHostFromToken(token);
  if (!tokenHost) return host;

  const normalizedHost = host.trim();
  return !normalizedHost || OFFICIAL_HOST_PATTERN.test(normalizedHost) ? tokenHost : host;
}

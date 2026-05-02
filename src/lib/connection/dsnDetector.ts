// SPDX-License-Identifier: Apache-2.0

import { Driver } from './drivers';

export interface DsnDetection {
  driver: Driver;
  /** What in the URL triggered the match (host pattern, scheme, etc.) */
  hint: string;
}

/**
 * Detect a managed-DB driver from a connection string the user just pasted.
 *
 * Returns `null` when no specific driver is identified — caller should keep
 * the previously selected driver. Detection is host-based (cheap regex), no
 * network round-trip. TimescaleDB cannot be detected from the URL alone and
 * is left to be discovered post-connection via `pg_extension`.
 */
export function detectDriverFromDsn(dsn: string): DsnDetection | null {
  const trimmed = dsn.trim();
  if (!trimmed) return null;

  if (/\.supabase\.(co|net)/i.test(trimmed) || /pooler\.supabase\.com/i.test(trimmed)) {
    return { driver: Driver.Supabase, hint: '*.supabase.co' };
  }

  if (/\.neon\.tech/i.test(trimmed)) {
    return { driver: Driver.Neon, hint: '*.neon.tech' };
  }

  return null;
}

export interface ParsedDsn {
  host: string;
  port?: number;
  username: string;
  password: string;
  database?: string;
  sslMode?: string;
}

/**
 * Best-effort PG/Redis-like connection string parser. Used after a successful
 * `detectDriverFromDsn` to pre-fill the connection form.
 */
export function parseDsn(dsn: string): ParsedDsn | null {
  try {
    const url = new URL(dsn.trim());
    const params = new URLSearchParams(url.search);
    const port = url.port ? Number(url.port) : undefined;
    const database = url.pathname.replace(/^\//, '') || undefined;
    return {
      host: url.hostname,
      port: Number.isFinite(port) ? port : undefined,
      username: decodeURIComponent(url.username || ''),
      password: decodeURIComponent(url.password || ''),
      database,
      sslMode: params.get('sslmode') ?? params.get('ssl_mode') ?? undefined,
    };
  } catch {
    return null;
  }
}

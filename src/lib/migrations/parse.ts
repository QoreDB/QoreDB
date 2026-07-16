// SPDX-License-Identifier: Apache-2.0

//! Parsing and building of the migration file format (dbmate-compatible):
//! one `.sql` file per migration with `-- migrate:up` / `-- migrate:down` sections.

import type { MigrationSummary } from './types';

const UP_MARKER = '-- migrate:up';
const DOWN_MARKER = '-- migrate:down';

/** Splits a migration file into its up and down sections. */
export function parseMigration(content: string): { up: string; down: string } {
  const upIdx = content.indexOf(UP_MARKER);
  const downIdx = content.indexOf(DOWN_MARKER);

  // No markers: treat the whole file as the up script (still runnable).
  if (upIdx === -1 && downIdx === -1) {
    return { up: content.trim(), down: '' };
  }

  const up =
    upIdx === -1
      ? ''
      : content.slice(upIdx + UP_MARKER.length, downIdx === -1 ? undefined : downIdx).trim();
  const down = downIdx === -1 ? '' : content.slice(downIdx + DOWN_MARKER.length).trim();

  return { up, down };
}

/** Serializes up/down scripts into a migration file body. */
export function serializeMigration(up: string, down: string): string {
  return `${UP_MARKER}\n${up.trim()}\n\n${DOWN_MARKER}\n${down.trim()}\n`;
}

/** Normalizes a human name into a filesystem-safe slug ([a-z0-9_-]). */
export function slugify(name: string): string {
  const slug = name
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, '_')
    .replace(/_+/g, '_')
    .replace(/^_|_$/g, '');
  return slug || 'migration';
}

/** Computes the next zero-padded version from the existing migrations. */
export function nextVersion(existing: MigrationSummary[]): string {
  const max = existing.reduce((acc, m) => {
    const n = Number.parseInt(m.version, 10);
    return Number.isFinite(n) && n > acc ? n : acc;
  }, 0);
  return String(max + 1).padStart(4, '0');
}

export function buildMigrationFilename(version: string, slug: string): string {
  return `${version}_${slug}.sql`;
}

/** Splits `<version>_<slug>.sql` into its parts. Mirrors the Rust `summarize`. */
export function summarize(filename: string): MigrationSummary {
  const stem = filename.endsWith('.sql') ? filename.slice(0, -4) : filename;
  const sep = stem.indexOf('_');
  const version = sep === -1 ? stem : stem.slice(0, sep);
  const name = sep === -1 ? stem : stem.slice(sep + 1);
  return { version, name, filename };
}

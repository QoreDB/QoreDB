// SPDX-License-Identifier: Apache-2.0

import { Driver } from '@/lib/drivers';
import { MONGO_TEMPLATES } from '../Editor/mongo-constants';

export function getDefaultQuery(isDocumentBased: boolean): string {
  return isDocumentBased ? MONGO_TEMPLATES.find : '';
}

export function getCollectionFromQuery(query: string): string {
  const trimmed = query.trim();
  if (trimmed.startsWith('{')) {
    try {
      const parsed = JSON.parse(trimmed);
      if (parsed && typeof parsed === 'object' && parsed.collection) {
        return String(parsed.collection);
      }
    } catch {
      return '';
    }
  }

  const directMatch = trimmed.match(/^db\.([a-zA-Z0-9_-]+)\./);
  if (directMatch) return directMatch[1];

  const getCollectionMatch = trimmed.match(/db\.getCollection\(['"]([^'"]+)['"]\)/);
  return getCollectionMatch ? getCollectionMatch[1] : '';
}

/**
 * Extract the target database from a USE statement.
 * Handles: USE db, USE `db`, USE "db", multi-statement queries (returns last USE).
 */
export function extractUseDatabase(query: string): string | null {
  const statements = query.split(';').map(s => s.trim()).filter(Boolean);
  let lastDb: string | null = null;
  for (const stmt of statements) {
    const match = stmt.match(/^use\s+[`"']?([^`"'\s;]+)[`"']?\s*$/i);
    if (match) lastDb = match[1];
  }
  return lastDb;
}

export function shouldRefreshSchema(
  queryToCheck: string,
  isDocumentBased: boolean,
  driver?: Driver
): boolean {
  if (!queryToCheck.trim()) return false;

  // Redis: always refresh
  if (driver === Driver.Redis) return true;

  if (isDocumentBased) {
    return (
      /\.createCollection\s*\(/i.test(queryToCheck) ||
      /\.dropDatabase\s*\(/i.test(queryToCheck) ||
      /\.drop\s*\(/i.test(queryToCheck) ||
      /\.renameCollection\s*\(/i.test(queryToCheck) ||
      /"operation"\s*:\s*"(create_collection|drop_collection|drop_database|rename_collection)"/i.test(
        queryToCheck
      )
    );
  }

  return /\b(CREATE|DROP|ALTER|TRUNCATE|RENAME)\b/i.test(queryToCheck);
}

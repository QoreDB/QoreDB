import { MONGO_TEMPLATES } from '../Editor/MongoEditor';

export function getDefaultQuery(isMongo: boolean): string {
  return isMongo ? MONGO_TEMPLATES.find : 'SELECT 1;'; //TODO : à améliorer, ce n'est pas assez universel
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

export function shouldRefreshSchema(queryToCheck: string, isMongo: boolean): boolean {
  if (!queryToCheck.trim()) return false;
  if (isMongo) {
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

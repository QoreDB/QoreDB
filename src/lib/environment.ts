/**
 * Environment utilities for connection classification
 */

export type Environment = 'development' | 'staging' | 'production';

export interface EnvironmentConfig {
  color: string;
  bgSoft: string;
  label: string;
  labelShort: string;
}

export const ENVIRONMENT_CONFIG: Record<Environment, EnvironmentConfig> = {
  development: {
    color: 'var(--q-env-dev)',
    bgSoft: 'var(--q-env-dev-soft)',
    label: 'Development',
    labelShort: 'DEV',
  },
  staging: {
    color: 'var(--q-env-staging)',
    bgSoft: 'var(--q-env-staging-soft)',
    label: 'Staging',
    labelShort: 'STG',
  },
  production: {
    color: 'var(--q-env-prod)',
    bgSoft: 'var(--q-env-prod-soft)',
    label: 'Production',
    labelShort: 'PROD',
  },
};

const SQL_MUTATION_KEYWORDS = new Set([
  'INSERT',
  'UPDATE',
  'DELETE',
  'DROP',
  'TRUNCATE',
  'ALTER',
  'CREATE',
  'REPLACE',
  'MERGE',
  'GRANT',
  'REVOKE',
  'CALL',
  'EXEC',
  'EXECUTE',
  'COPY',
]);

/**
 * Dangerous SQL patterns that require confirmation in production
 */
const DANGEROUS_PATTERNS = [
  /^\s*DROP\s+(TABLE|DATABASE|SCHEMA|INDEX|VIEW|FUNCTION|TRIGGER)\b/i,
  /^\s*TRUNCATE\b/i,
  /^\s*DELETE\s+FROM\b/i,
  /^\s*UPDATE\b/i,
  /^\s*ALTER\s+TABLE\b.*\bDROP\b/i,
  /^\s*DROP\s+ALL\b/i,
];

const SQL_IDENTIFIER = '(?:"[^"]+"|`[^`]+`|[^\\s;]+)';
const SQL_QUALIFIED_IDENTIFIER = `${SQL_IDENTIFIER}(?:\\.${SQL_IDENTIFIER})*`;
const SQL_TARGET_PATTERNS: RegExp[] = [
  new RegExp(
    `^\\s*DROP\\s+(?:TABLE|DATABASE|SCHEMA|INDEX|VIEW|FUNCTION|TRIGGER)\\s+(?:IF\\s+EXISTS\\s+)?(${SQL_QUALIFIED_IDENTIFIER})`,
    'i'
  ),
  new RegExp(`^\\s*TRUNCATE\\s+(?:TABLE\\s+)?(${SQL_QUALIFIED_IDENTIFIER})`, 'i'),
  new RegExp(`^\\s*DELETE\\s+FROM\\s+(${SQL_QUALIFIED_IDENTIFIER})`, 'i'),
  new RegExp(`^\\s*UPDATE\\s+(${SQL_QUALIFIED_IDENTIFIER})`, 'i'),
  new RegExp(`^\\s*ALTER\\s+TABLE\\s+(${SQL_QUALIFIED_IDENTIFIER})`, 'i'),
];

const MONGO_MUTATION_PATTERNS = [
  /\.insert(?:one|many)?\s*\(/i,
  /\.update(?:one|many)?\s*\(/i,
  /\.replaceOne\s*\(/i,
  /\.delete(?:one|many)?\s*\(/i,
  /\.remove\s*\(/i,
  /\.createCollection\s*\(/i,
  /\.drop(?:Database)?\s*\(/i,
  /\.bulkWrite\s*\(/i,
  /\.findOneAnd(?:Update|Delete|Replace)\s*\(/i,
  /"operation"\s*:\s*"(create_collection|drop_collection|drop_database)"/i,
];

function normalizeSql(sql: string): string {
  return sql
    .replace(/--.*$/gm, ' ')
    .replace(/\/\*[\s\S]*?\*\//g, ' ')
    .replace(/'(?:''|[^'])*'/g, "''")
    .replace(/\"(?:\"\"|[^\"])*\"/g, '""');
}

function splitSqlStatements(sql: string): string[] {
  return normalizeSql(sql)
    .split(';')
    .map(statement => statement.trim())
    .filter(Boolean);
}

function splitRawSqlStatements(sql: string): string[] {
  return sql
    .split(';')
    .map(statement => statement.trim())
    .filter(Boolean);
}

function normalizeSqlTarget(target: string): string {
  const trimmed = target.trim().replace(/;$/, '');
  const first = trimmed.split(',')[0]?.trim() || '';
  return first.replace(/["`]/g, '');
}

function tokenizeSql(sql: string): string[] {
  return normalizeSql(sql)
    .split(/[^A-Za-z0-9_]+/)
    .map(token => token.trim())
    .filter(Boolean)
    .map(token => token.toUpperCase());
}

export type QueryDialect = 'sql' | 'mongodb';

export function isMutationQuery(query: string, dialect: QueryDialect = 'sql'): boolean {
  if (!query.trim()) return false;

  if (dialect === 'mongodb') {
    return MONGO_MUTATION_PATTERNS.some(pattern => pattern.test(query));
  }

  return tokenizeSql(query).some(token => SQL_MUTATION_KEYWORDS.has(token));
}

/**
 * Checks if a SQL query contains potentially dangerous patterns
 */
export function isDangerousQuery(sql: string): boolean {
  return splitSqlStatements(sql).some(statement => {
    if (DANGEROUS_PATTERNS.some(pattern => pattern.test(statement))) {
      if (/^\s*DELETE\s+FROM\b/i.test(statement) && /\bWHERE\b/i.test(statement)) {
        return false;
      }
      if (/^\s*UPDATE\b/i.test(statement) && /\bWHERE\b/i.test(statement)) {
        return false;
      }
      return true;
    }
    return false;
  });
}

export function isDropDatabaseQuery(sql: string): boolean {
  return splitSqlStatements(sql).some(statement => /^\s*DROP\s+DATABASE\b/i.test(statement));
}

/**
 * Get a human-readable description of why a query is dangerous
 */
export function getDangerousQueryReason(sql: string): string | null {
  for (const statement of splitSqlStatements(sql)) {
    if (/^\s*DROP\s+(TABLE|DATABASE|SCHEMA|INDEX|VIEW|FUNCTION|TRIGGER)\b/i.test(statement)) {
      return 'This query will permanently delete data structures';
    }
    if (/^\s*TRUNCATE\b/i.test(statement)) {
      return 'This query will delete all rows from the table';
    }
    if (/^\s*DELETE\s+FROM\b/i.test(statement) && !/\bWHERE\b/i.test(statement)) {
      return 'DELETE without WHERE clause will remove all rows';
    }
    if (/^\s*UPDATE\b/i.test(statement) && !/\bWHERE\b/i.test(statement)) {
      return 'UPDATE without WHERE clause will modify all rows';
    }
    if (/^\s*ALTER\s+TABLE\b.*\bDROP\b/i.test(statement)) {
      return 'This query will drop columns or constraints';
    }
  }

  return null;
}

export function getDangerousQueryTarget(sql: string): string | null {
  for (const statement of splitRawSqlStatements(sql)) {
    for (const pattern of SQL_TARGET_PATTERNS) {
      const match = statement.match(pattern);
      if (match && match[1]) {
        const target = normalizeSqlTarget(match[1]);
        if (target) {
          return target;
        }
      }
    }
  }

  return null;
}

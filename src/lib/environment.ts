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
    color: '#16A34A',
    bgSoft: 'rgba(22, 163, 74, 0.15)',
    label: 'Development',
    labelShort: 'DEV',
  },
  staging: {
    color: '#F59E0B',
    bgSoft: 'rgba(245, 158, 11, 0.15)',
    label: 'Staging',
    labelShort: 'STG',
  },
  production: {
    color: '#DC2626',
    bgSoft: 'rgba(220, 38, 38, 0.15)',
    label: 'Production',
    labelShort: 'PROD',
  },
};

/**
 * Dangerous SQL patterns that require confirmation in production
 */
const DANGEROUS_PATTERNS = [
  /\bDROP\s+(TABLE|DATABASE|SCHEMA|INDEX|VIEW|FUNCTION|TRIGGER)/i,
  /\bTRUNCATE\s+(TABLE\s+)?\w+/i,
  /\bDELETE\s+FROM\s+\w+\s*(?:;|$)/i, // DELETE without WHERE
  /\bALTER\s+TABLE\s+\w+\s+DROP/i,
  /\bDROP\s+ALL\b/i,
];

/**
 * Checks if a SQL query contains potentially dangerous patterns
 */
export function isDangerousQuery(sql: string): boolean {
  const normalized = sql.trim();
  return DANGEROUS_PATTERNS.some(pattern => pattern.test(normalized));
}

/**
 * Get a human-readable description of why a query is dangerous
 */
export function getDangerousQueryReason(sql: string): string | null {
  
  if (/\bDROP\s+(TABLE|DATABASE|SCHEMA)/i.test(sql)) {
    return 'This query will permanently delete data structures';
  }
  if (/\bTRUNCATE/i.test(sql)) {
    return 'This query will delete all rows from the table';
  }
  if (/\bDELETE\s+FROM\s+\w+\s*(?:;|$)/i.test(sql)) {
    return 'DELETE without WHERE clause will remove all rows';
  }
  if (/\bALTER\s+TABLE\s+\w+\s+DROP/i.test(sql)) {
    return 'This query will drop columns or constraints';
  }
  
  return null;
}

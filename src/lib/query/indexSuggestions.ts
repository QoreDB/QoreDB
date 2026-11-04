// SPDX-License-Identifier: BUSL-1.1

import { isMySqlFamily } from '@/lib/connection/driverCapabilities';
import { Driver } from '@/lib/connection/drivers';
import { quoteIdentifier } from '@/lib/ddl';
import type { PlanNode } from './explainPlanParser';

export interface IndexSuggestion {
  /** Table the index would be created on (unqualified, as reported by the plan). */
  table: string;
  columns: string[];
  reason: 'seqScan' | 'fullTableScan';
  cost?: number;
  estimatedRows?: number;
  indexName: string;
  sql: string;
}

/** Only full scans above one of these thresholds warrant an index — small tables
 *  are scanned regardless and an index would not help. */
const MIN_COST = 100;
const MIN_ROWS = 1000;
const MAX_COLUMNS = 4;
const MAX_SUGGESTIONS = 5;

/** PG renders LIKE/ILIKE as ~~ / ~~* and their negations in plan filters. */
const SYMBOL_OP = /([`"\w.]+)\s*(<=|>=|<>|!=|=|<|>|!~~\*?|~~\*?)/g;
const WORD_OP = /([`"\w.]+)\s+(?:like|ilike|in|between|is)\b/gi;
const VALID_COLUMN = /^[A-Za-z_][\w$]*$/;

const STOPWORDS = new Set([
  'and',
  'or',
  'not',
  'null',
  'true',
  'false',
  'any',
  'all',
  'case',
  'when',
  'then',
  'else',
  'end',
  'is',
  'in',
  'like',
  'ilike',
  'between',
  'exists',
]);

function stripCastsAndLiterals(filter: string): string {
  return filter
    .replace(/::\s*"?[A-Za-z_][\w ]*"?(\[\])?/g, '') // PG type casts: ::text, ::timestamp without time zone[]
    .replace(/'(?:[^']|'')*'/g, '?'); // string literals
}

function normalizeColumn(raw: string): string | null {
  const stripped = raw.replace(/[`"]/g, '');
  const segments = stripped.split('.');
  const column = segments[segments.length - 1];
  if (!column || STOPWORDS.has(column.toLowerCase()) || !VALID_COLUMN.test(column)) {
    return null;
  }
  return column;
}

function extractFilterColumns(filter: string): string[] {
  const cleaned = stripCastsAndLiterals(filter);
  const seen = new Set<string>();
  const columns: string[] = [];

  const collect = (match: RegExpMatchArray) => {
    const column = normalizeColumn(match[1]);
    if (!column) return;
    const key = column.toLowerCase();
    if (seen.has(key)) return;
    seen.add(key);
    columns.push(column);
  };

  for (const match of cleaned.matchAll(SYMBOL_OP)) collect(match);
  for (const match of cleaned.matchAll(WORD_OP)) collect(match);

  return columns.slice(0, MAX_COLUMNS);
}

function isFullScan(node: PlanNode): IndexSuggestion['reason'] | null {
  const type = node.nodeType.toLowerCase();
  if (type === 'seq scan') return 'seqScan';
  if (type === 'all') return 'fullTableScan'; // MySQL access_type=ALL
  return null;
}

function buildIndexName(table: string, columns: string[]): string {
  return `idx_${table}_${columns.join('_')}`
    .replace(/[^A-Za-z0-9_]/g, '_')
    .replace(/_+/g, '_')
    .toLowerCase()
    .slice(0, 60);
}

function buildCreateIndexSql(
  table: string,
  columns: string[],
  indexName: string,
  driver: Driver
): string {
  const cols = columns.map(col => quoteIdentifier(col, driver)).join(', ');
  return `CREATE INDEX ${quoteIdentifier(indexName, driver)} ON ${quoteIdentifier(table, driver)} (${cols});`;
}

function walk(node: PlanNode, visit: (node: PlanNode) => void): void {
  visit(node);
  for (const child of node.children) walk(child, visit);
}

/**
 * Derive CREATE INDEX suggestions from a parsed EXPLAIN plan: each costly full
 * table scan that filters on bare column references becomes a candidate index on
 * those columns. Function-wrapped or computed predicates are skipped — a plain
 * index would not cover them. PostgreSQL and MySQL/MariaDB only.
 */
export function suggestIndexesFromPlan(root: PlanNode, driver: Driver): IndexSuggestion[] {
  const byKey = new Map<string, IndexSuggestion>();

  walk(root, node => {
    const reason = isFullScan(node);
    if (!reason || !node.relation || !node.filter) return;

    const cost = node.totalCost;
    const estimatedRows = node.actualRows ?? node.planRows;
    const costly = cost === undefined || cost >= MIN_COST || (estimatedRows ?? 0) >= MIN_ROWS;
    if (!costly) return;

    const columns = extractFilterColumns(node.filter);
    if (columns.length === 0) return;

    const table = node.relation;
    const key = `${table.toLowerCase()}(${columns.map(c => c.toLowerCase()).join(',')})`;
    const existing = byKey.get(key);
    if (existing && (existing.cost ?? 0) >= (cost ?? 0)) return;

    const indexName = buildIndexName(table, columns);
    byKey.set(key, {
      table,
      columns,
      reason,
      cost,
      estimatedRows,
      indexName,
      sql: buildCreateIndexSql(table, columns, indexName, driver),
    });
  });

  return Array.from(byKey.values())
    .sort((a, b) => (b.cost ?? 0) - (a.cost ?? 0))
    .slice(0, MAX_SUGGESTIONS);
}

const FROM_TABLE_RE = /\bfrom\s+([`"[\]\w.]+)/i;
const WHERE_CLAUSE_RE =
  /\bwhere\s+([\s\S]+?)(?:\bgroup\s+by\b|\border\s+by\b|\blimit\b|\boffset\b|;|$)/i;
const ORDER_BY_RE = /\border\s+by\s+([\s\S]+?)(?:\blimit\b|\boffset\b|;|$)/i;

function extractTableFromSql(sql: string): string | null {
  const match = sql.match(FROM_TABLE_RE);
  if (!match) return null;
  const raw = match[1].replace(/[`"[\]]/g, '');
  const parts = raw.split('.').filter(Boolean);
  const table = parts[parts.length - 1];
  if (!table || STOPWORDS.has(table.toLowerCase()) || !VALID_COLUMN.test(table)) {
    return null;
  }
  return table;
}

function extractOrderByColumns(orderByClause: string): string[] {
  const cols: string[] = [];
  for (const part of orderByClause.split(',')) {
    const token = part
      .trim()
      .replace(/\b(?:asc|desc|nulls\s+first|nulls\s+last)\b/gi, '')
      .trim();
    const normalized = normalizeColumn(token);
    if (normalized) {
      cols.push(normalized);
    }
  }
  return cols;
}

/**
 * Derive a heuristic CREATE INDEX suggestion from a raw SQL query (used in Query Trends,
 * Slow Queries, and text EXPLAIN plans where a structured tree plan is not available).
 */
export function suggestIndexesFromSqlQuery(
  sql: string,
  driver: Driver = Driver.Postgres
): IndexSuggestion[] {
  const cleanedSql = sql.replace(/^explain(?:\s*\([^)]*\)|\s+analyze|\s+query\s+plan)*\s+/i, '');
  const table = extractTableFromSql(cleanedSql);
  if (!table) return [];

  const seen = new Set<string>();
  const columns: string[] = [];
  const pushCol = (col: string) => {
    const key = col.toLowerCase();
    if (!seen.has(key) && columns.length < MAX_COLUMNS) {
      seen.add(key);
      columns.push(col);
    }
  };

  const whereMatch = cleanedSql.match(WHERE_CLAUSE_RE);
  if (whereMatch) {
    for (const col of extractFilterColumns(whereMatch[1])) {
      pushCol(col);
    }
  }

  const orderMatch = cleanedSql.match(ORDER_BY_RE);
  if (orderMatch) {
    for (const col of extractOrderByColumns(orderMatch[1])) {
      pushCol(col);
    }
  }

  if (columns.length === 0) return [];

  const indexName = buildIndexName(table, columns);
  return [
    {
      table,
      columns,
      reason: 'fullTableScan',
      indexName,
      sql: buildCreateIndexSql(table, columns, indexName, driver),
    },
  ];
}

/** EXPLAIN-plan and SQL index suggestions cover relational SQL engines. */
export function supportsIndexSuggestions(driver: Driver): boolean {
  return (
    driver === Driver.Postgres ||
    isMySqlFamily(driver) ||
    driver === Driver.Sqlite ||
    driver === Driver.SqlServer ||
    driver === Driver.AzureSql ||
    driver === Driver.Cockroachdb ||
    driver === Driver.YugabyteDb ||
    driver === Driver.Supabase ||
    driver === Driver.Neon ||
    driver === Driver.Timescaledb
  );
}

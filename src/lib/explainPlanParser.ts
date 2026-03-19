// SPDX-License-Identifier: Apache-2.0

import type { QueryResult } from './tauri';

/**
 * Normalized tree node from an EXPLAIN plan (PostgreSQL or MySQL).
 */
export interface PlanNode {
  id: string;
  nodeType: string;
  relation?: string;
  alias?: string;
  startupCost?: number;
  totalCost?: number;
  planRows?: number;
  planWidth?: number;
  actualStartupTime?: number;
  actualTotalTime?: number;
  actualRows?: number;
  actualLoops?: number;
  filter?: string;
  indexName?: string;
  sortKey?: string[];
  children: PlanNode[];
  extra: Record<string, unknown>;
}

let nodeIdCounter = 0;
function nextId(): string {
  return `plan-${++nodeIdCounter}`;
}

// PG EXPLAIN keys we extract into named fields
const KNOWN_PG_KEYS = new Set([
  'Node Type',
  'Relation Name',
  'Alias',
  'Startup Cost',
  'Total Cost',
  'Plan Rows',
  'Plan Width',
  'Actual Startup Time',
  'Actual Total Time',
  'Actual Rows',
  'Actual Loops',
  'Filter',
  'Index Name',
  'Sort Key',
  'Plans',
]);

/** Helper to read a key from a Record and cast it. */
function get<T>(obj: Record<string, unknown>, key: string): T | undefined {
  return obj[key] as T | undefined;
}

/**
 * Extract the raw plan value from a QueryResult.
 */
function extractPlanData(result: QueryResult): { json: unknown } | { text: string } | null {
  if (!result.rows.length) return null;

  const firstValue = result.rows[0]?.values?.[0];
  if (firstValue !== null && firstValue !== undefined) {
    if (typeof firstValue === 'object') {
      return { json: firstValue };
    }
    if (typeof firstValue === 'string') {
      try {
        return { json: JSON.parse(firstValue) };
      } catch {
        // Not JSON — fall through to text
      }
    }
  }

  const lines = result.rows.map(r => String(r.values[0] ?? '')).filter(Boolean);
  if (lines.length > 0) return { text: lines.join('\n') };

  return null;
}

/**
 * Parse a PostgreSQL JSON EXPLAIN plan node.
 */
function parsePgNode(raw: Record<string, unknown>): PlanNode {
  const extra: Record<string, unknown> = {};
  for (const [key, value] of Object.entries(raw)) {
    if (!KNOWN_PG_KEYS.has(key)) {
      extra[key] = value;
    }
  }

  const childPlans = get<Record<string, unknown>[]>(raw, 'Plans');

  return {
    id: nextId(),
    nodeType: String(get(raw, 'Node Type') ?? 'Unknown'),
    relation: get<string>(raw, 'Relation Name'),
    alias: get<string>(raw, 'Alias'),
    startupCost: get<number>(raw, 'Startup Cost'),
    totalCost: get<number>(raw, 'Total Cost'),
    planRows: get<number>(raw, 'Plan Rows'),
    planWidth: get<number>(raw, 'Plan Width'),
    actualStartupTime: get<number>(raw, 'Actual Startup Time'),
    actualTotalTime: get<number>(raw, 'Actual Total Time'),
    actualRows: get<number>(raw, 'Actual Rows'),
    actualLoops: get<number>(raw, 'Actual Loops'),
    filter: get<string>(raw, 'Filter'),
    indexName: get<string>(raw, 'Index Name'),
    sortKey: get<string[]>(raw, 'Sort Key'),
    children: childPlans ? childPlans.map(parsePgNode) : [],
    extra,
  };
}

/**
 * Parse MySQL JSON EXPLAIN format.
 */
function parseMysqlNode(raw: Record<string, unknown>, label?: string): PlanNode {
  const children: PlanNode[] = [];
  const extra: Record<string, unknown> = {};

  const nestedLoop = get<Record<string, unknown>[]>(raw, 'nested_loop');
  if (nestedLoop) {
    for (const item of nestedLoop) {
      const table = get<Record<string, unknown>>(item, 'table');
      if (table) children.push(parseMysqlTable(table));
    }
  }

  for (const opKey of ['ordering_operation', 'grouping_operation', 'duplicates_removal']) {
    const op = get<Record<string, unknown>>(raw, opKey);
    if (op) children.push(parseMysqlNode(op, opKey.replace(/_/g, ' ')));
  }

  const subqueries = get<Record<string, unknown>[]>(raw, 'subqueries');
  if (subqueries) {
    for (const sq of subqueries) {
      children.push(parseMysqlNode(sq, 'subquery'));
    }
  }

  const table = get<Record<string, unknown>>(raw, 'table');
  if (table && !nestedLoop) {
    children.push(parseMysqlTable(table));
  }

  const costInfo = get<Record<string, string>>(raw, 'cost_info');
  const queryCost = costInfo?.query_cost ?? costInfo?.prefix_cost;

  const skipKeys = new Set([
    'nested_loop',
    'ordering_operation',
    'grouping_operation',
    'duplicates_removal',
    'subqueries',
    'table',
    'cost_info',
    'select_id',
    'query_block',
  ]);
  for (const [key, value] of Object.entries(raw)) {
    if (!skipKeys.has(key) && typeof value !== 'object') {
      extra[key] = value;
    }
  }

  return {
    id: nextId(),
    nodeType: label || 'query_block',
    totalCost: queryCost ? parseFloat(queryCost) : undefined,
    children,
    extra,
  };
}

function parseMysqlTable(raw: Record<string, unknown>): PlanNode {
  const costInfo = get<Record<string, string>>(raw, 'cost_info');
  const extra: Record<string, unknown> = {};

  const skipKeys = new Set([
    'table_name',
    'access_type',
    'rows_examined_per_scan',
    'rows_produced_per_join',
    'cost_info',
    'key',
    'used_key_parts',
    'filtered',
    'attached_condition',
  ]);
  for (const [key, value] of Object.entries(raw)) {
    if (!skipKeys.has(key) && typeof value !== 'object') {
      extra[key] = value;
    }
  }

  return {
    id: nextId(),
    nodeType: String(get(raw, 'access_type') ?? 'ALL'),
    relation: get<string>(raw, 'table_name'),
    indexName: get<string>(raw, 'key'),
    planRows: get<number>(raw, 'rows_examined_per_scan'),
    totalCost: costInfo?.prefix_cost ? parseFloat(costInfo.prefix_cost) : undefined,
    filter: get<string>(raw, 'attached_condition'),
    children: [],
    extra,
  };
}

/**
 * Parse a QueryResult into a plan tree, or return the raw text.
 */
export function parseExplainPlan(
  result: QueryResult
): { type: 'tree'; root: PlanNode; rootCost: number } | { type: 'text'; text: string } | null {
  nodeIdCounter = 0;
  const data = extractPlanData(result);
  if (!data) return null;

  if ('text' in data) {
    return { type: 'text', text: data.text };
  }

  const json = data.json;

  // PostgreSQL format: [{ "Plan": { ... } }]
  if (Array.isArray(json) && json.length > 0 && json[0]?.Plan) {
    const root = parsePgNode(json[0].Plan as Record<string, unknown>);
    return { type: 'tree', root, rootCost: computeMaxCost(root) };
  }

  // PostgreSQL: Plan as top-level object
  const jsonObj = json as Record<string, unknown> | null;
  if (jsonObj && typeof jsonObj === 'object' && !Array.isArray(jsonObj)) {
    const plan = get<Record<string, unknown>>(jsonObj, 'Plan');
    if (plan) {
      const root = parsePgNode(plan);
      return { type: 'tree', root, rootCost: computeMaxCost(root) };
    }

    // MySQL format: { "query_block": { ... } }
    const queryBlock = get<Record<string, unknown>>(jsonObj, 'query_block');
    if (queryBlock) {
      const root = parseMysqlNode(queryBlock);
      return { type: 'tree', root, rootCost: computeMaxCost(root) };
    }
  }

  return { type: 'text', text: JSON.stringify(json, null, 2) };
}

function computeMaxCost(node: PlanNode): number {
  let max = node.totalCost ?? 0;
  for (const child of node.children) {
    max = Math.max(max, computeMaxCost(child));
  }
  return max;
}

/**
 * Get a text color class representing node cost relative to max.
 */
export function getCostColor(cost: number | undefined, maxCost: number): string {
  if (cost === undefined || maxCost <= 0) return 'text-muted-foreground';
  const ratio = cost / maxCost;
  if (ratio < 0.2) return 'text-green-500';
  if (ratio < 0.4) return 'text-green-400 dark:text-green-500';
  if (ratio < 0.6) return 'text-amber-500';
  if (ratio < 0.8) return 'text-orange-500';
  return 'text-red-500';
}

/**
 * Get a background color class for the cost bar.
 */
export function getCostBarColor(cost: number | undefined, maxCost: number): string {
  if (cost === undefined || maxCost <= 0) return 'bg-muted';
  const ratio = cost / maxCost;
  if (ratio < 0.2) return 'bg-green-500/20';
  if (ratio < 0.4) return 'bg-green-400/20 dark:bg-green-500/20';
  if (ratio < 0.6) return 'bg-amber-500/20';
  if (ratio < 0.8) return 'bg-orange-500/25';
  return 'bg-red-500/25';
}

/**
 * Get the cost bar width percentage.
 */
export function getCostBarWidth(cost: number | undefined, maxCost: number): number {
  if (cost === undefined || maxCost <= 0) return 0;
  return Math.max(2, Math.round((cost / maxCost) * 100));
}

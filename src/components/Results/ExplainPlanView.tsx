// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, ChevronRight, Database, GitBranch, Search, Table2 } from 'lucide-react';
import { useCallback, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  getCostBarColor,
  getCostBarWidth,
  getCostColor,
  type PlanNode,
  parseExplainPlan,
} from '@/lib/query/explainPlanParser';
import type { QueryResult } from '@/lib/tauri';
import { cn } from '@/lib/utils';

interface ExplainPlanViewProps {
  result: QueryResult;
}

export function ExplainPlanView({ result }: ExplainPlanViewProps) {
  const { t } = useTranslation();
  const plan = useMemo(() => parseExplainPlan(result), [result]);

  if (!plan) {
    return (
      <div className="flex items-center justify-center h-full text-muted-foreground text-sm">
        {t('query.explainNoPlan')}
      </div>
    );
  }

  if (plan.type === 'text') {
    return (
      <div className="flex-1 overflow-auto p-3">
        <pre className="text-xs font-mono whitespace-pre-wrap wrap-break-word">{plan.text}</pre>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto p-3">
      <div className="space-y-0.5">
        <PlanTreeNode node={plan.root} maxCost={plan.rootCost} depth={0} defaultExpanded />
      </div>
    </div>
  );
}

// --- Tree node component ---

interface PlanTreeNodeProps {
  node: PlanNode;
  maxCost: number;
  depth: number;
  defaultExpanded?: boolean;
}

function PlanTreeNode({ node, maxCost, depth, defaultExpanded = false }: PlanTreeNodeProps) {
  const [expanded, setExpanded] = useState(defaultExpanded || depth < 3);
  const hasChildren = node.children.length > 0;

  const toggle = useCallback(() => setExpanded(prev => !prev), []);

  const costColor = getCostColor(node.totalCost, maxCost);
  const barColor = getCostBarColor(node.totalCost, maxCost);
  const barWidth = getCostBarWidth(node.totalCost, maxCost);

  return (
    <div className={cn(depth > 0 && 'ml-4 border-l border-border/50 pl-1')}>
      {/* Node header */}
      <button
        type="button"
        onClick={toggle}
        className={cn(
          'group flex items-center gap-1.5 w-full text-left rounded-md px-2 py-1 text-xs',
          'hover:bg-muted/60 transition-colors relative overflow-hidden'
        )}
      >
        {/* Cost bar background */}
        {node.totalCost !== undefined && (
          <div
            className={cn('absolute inset-y-0 left-0 transition-all', barColor)}
            style={{ width: `${barWidth}%` }}
          />
        )}

        {/* Content (above the bar) */}
        <div className="relative flex items-center gap-1.5 flex-1 min-w-0 z-10">
          {/* Expand/collapse chevron */}
          {hasChildren ? (
            expanded ? (
              <ChevronDown size={12} className="shrink-0 text-muted-foreground" />
            ) : (
              <ChevronRight size={12} className="shrink-0 text-muted-foreground" />
            )
          ) : (
            <span className="w-3 shrink-0" />
          )}

          {/* Node type icon */}
          <NodeIcon nodeType={node.nodeType} />

          {/* Node type label */}
          <span className="font-semibold text-foreground truncate">{node.nodeType}</span>

          {/* Relation/table name */}
          {node.relation && (
            <span className="text-accent truncate">
              {node.alias && node.alias !== node.relation
                ? `${node.relation} (${node.alias})`
                : node.relation}
            </span>
          )}

          {/* Index name */}
          {node.indexName && (
            <span className="text-muted-foreground truncate">idx: {node.indexName}</span>
          )}

          {/* Spacer */}
          <span className="flex-1" />

          {/* Metrics pills */}
          <div className="flex items-center gap-2 shrink-0">
            {node.totalCost !== undefined && (
              <span className={cn('font-mono tabular-nums', costColor)} title="Total cost">
                {formatCost(node.totalCost)}
              </span>
            )}
            {node.planRows !== undefined && (
              <span className="font-mono tabular-nums text-muted-foreground" title="Estimated rows">
                {formatNumber(node.planRows)} rows
              </span>
            )}
            {node.actualRows !== undefined && (
              <span
                className="font-mono tabular-nums text-blue-500 dark:text-blue-400"
                title="Actual rows"
              >
                ({formatNumber(node.actualRows)} actual)
              </span>
            )}
            {node.actualTotalTime !== undefined && (
              <span
                className="font-mono tabular-nums text-muted-foreground"
                title="Actual time (ms)"
              >
                {node.actualTotalTime.toFixed(2)}ms
              </span>
            )}
          </div>
        </div>
      </button>

      {/* Expanded details */}
      {expanded && (
        <>
          <NodeDetails node={node} depth={depth} />
          {hasChildren && (
            <div className="space-y-0.5">
              {node.children.map(child => (
                <PlanTreeNode key={child.id} node={child} maxCost={maxCost} depth={depth + 1} />
              ))}
            </div>
          )}
        </>
      )}
    </div>
  );
}

// --- Detail row for extra properties ---

function NodeDetails({ node, depth }: { node: PlanNode; depth: number }) {
  const details: Array<{ label: string; value: string }> = [];

  if (node.filter) details.push({ label: 'Filter', value: node.filter });
  if (node.sortKey?.length) details.push({ label: 'Sort Key', value: node.sortKey.join(', ') });
  if (node.startupCost !== undefined)
    details.push({ label: 'Startup Cost', value: formatCost(node.startupCost) });
  if (node.planWidth !== undefined) details.push({ label: 'Width', value: `${node.planWidth}` });
  if (node.actualLoops !== undefined && node.actualLoops > 1) {
    details.push({ label: 'Loops', value: `${node.actualLoops}` });
  }

  // Extra properties
  for (const [key, value] of Object.entries(node.extra)) {
    if (value === null || value === undefined) continue;
    const strValue = typeof value === 'object' ? JSON.stringify(value) : String(value);
    details.push({ label: formatLabel(key), value: strValue });
  }

  if (details.length === 0) return null;

  return (
    <div className={cn('ml-4 pl-1', depth > 0 && 'ml-4 border-l border-border/50 pl-1')}>
      <div className="ml-5 mb-1 flex flex-wrap gap-x-3 gap-y-0.5 text-[11px]">
        {details.map(({ label, value }) => (
          <span key={label} className="text-muted-foreground">
            <span className="font-medium">{label}:</span>{' '}
            <span className="text-foreground/80">{value}</span>
          </span>
        ))}
      </div>
    </div>
  );
}

// --- Node type icon ---

function NodeIcon({ nodeType }: { nodeType: string }) {
  const lower = nodeType.toLowerCase();

  if (lower.includes('scan') || lower.includes('all')) {
    return <Table2 size={12} className="shrink-0 text-muted-foreground" />;
  }
  if (lower.includes('index') || lower.includes('search')) {
    return <Search size={12} className="shrink-0 text-muted-foreground" />;
  }
  if (
    lower.includes('join') ||
    lower.includes('nested') ||
    lower.includes('merge') ||
    lower.includes('hash')
  ) {
    return <GitBranch size={12} className="shrink-0 text-muted-foreground" />;
  }
  return <Database size={12} className="shrink-0 text-muted-foreground" />;
}

// --- Formatting helpers ---

function formatCost(cost: number): string {
  if (cost >= 1_000_000) return `${(cost / 1_000_000).toFixed(1)}M`;
  if (cost >= 1_000) return `${(cost / 1_000).toFixed(1)}K`;
  return cost.toFixed(2);
}

function formatNumber(n: number): string {
  if (n >= 1_000_000) return `${(n / 1_000_000).toFixed(1)}M`;
  if (n >= 1_000) return `${(n / 1_000).toFixed(1)}K`;
  return String(n);
}

function formatLabel(key: string): string {
  // Convert "CamelCase" or "snake_case" to readable label
  return key
    .replace(/([a-z])([A-Z])/g, '$1 $2')
    .replace(/_/g, ' ')
    .replace(/\b\w/g, c => c.toUpperCase());
}

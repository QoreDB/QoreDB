// SPDX-License-Identifier: BUSL-1.1

import {
  AlertCircle,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Loader2,
  Wrench,
} from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DataGrid } from '@/components/Grid/DataGrid';
import type { AgentChatItem } from '@/hooks/useAgentChat';
import type { QueryResult } from '@/lib/tauri';

type ToolItem = Extract<AgentChatItem, { kind: 'tool' }>;

interface ToolStepCardProps {
  item: ToolItem;
}

/** Payload shape produced by the backend tool executor for tabular results. */
interface TabularPayload {
  columns: string[];
  rows: unknown[][];
  row_count: number;
  truncated?: boolean;
}

function parseTabular(content: string): TabularPayload | null {
  try {
    const parsed = JSON.parse(content) as TabularPayload;
    if (Array.isArray(parsed.columns) && Array.isArray(parsed.rows)) {
      return parsed;
    }
  } catch {
    // Not JSON.
  }
  return null;
}

export function ToolStepCard({ item }: ToolStepCardProps) {
  const { t } = useTranslation();
  const [expanded, setExpanded] = useState(false);

  const query = (item.input as { query?: string } | undefined)?.query;
  const tabular = useMemo(
    () => (item.result && !item.result.isError ? parseTabular(item.result.content) : null),
    [item.result]
  );

  const gridResult: QueryResult | null = useMemo(() => {
    if (!tabular) return null;
    return {
      columns: tabular.columns.map(name => ({ name, data_type: '', nullable: true })),
      rows: tabular.rows.map(values => ({ values: values as QueryResult['rows'][0]['values'] })),
      execution_time_ms: 0,
    };
  }, [tabular]);

  return (
    <div className="rounded-lg border border-border bg-muted/30 text-sm">
      <button
        type="button"
        onClick={() => setExpanded(v => !v)}
        className="flex w-full items-center gap-2 px-3 py-2 text-left"
      >
        {expanded ? (
          <ChevronDown size={14} className="shrink-0 text-muted-foreground" />
        ) : (
          <ChevronRight size={14} className="shrink-0 text-muted-foreground" />
        )}
        <Wrench size={13} className="shrink-0 text-muted-foreground" />
        <span className="font-mono text-xs">{item.name}</span>
        {query && <span className="truncate font-mono text-xs text-muted-foreground">{query}</span>}
        <span className="ml-auto shrink-0">
          {!item.result ? (
            <Loader2 size={13} className="animate-spin text-muted-foreground" />
          ) : item.result.isError ? (
            <AlertCircle size={13} className="text-destructive" />
          ) : (
            <CheckCircle2 size={13} className="text-green-600 dark:text-green-500" />
          )}
        </span>
      </button>
      {expanded && item.result && (
        <div className="border-t border-border px-3 py-2">
          {gridResult ? (
            <div className="overflow-hidden rounded" style={{ maxHeight: 320 }}>
              <DataGrid result={gridResult} readOnly environment="development" />
              {tabular?.truncated && (
                <div className="py-1 text-xs text-muted-foreground">
                  {t('agentChat.tool.truncated', { count: tabular.row_count })}
                </div>
              )}
            </div>
          ) : (
            <pre className="max-h-60 overflow-auto whitespace-pre-wrap break-all font-mono text-xs text-muted-foreground">
              {item.result.content}
            </pre>
          )}
        </div>
      )}
    </div>
  );
}

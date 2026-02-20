// SPDX-License-Identifier: BUSL-1.1

import {
  AlertCircle,
  CheckCircle2,
  Code,
  Database,
  Loader2,
  Play,
  Server,
  Table2,
} from 'lucide-react';
/**
 * DiffSourcePanel - Panel for selecting a data source (table or query)
 */
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Textarea } from '@/components/ui/textarea';
import type { Namespace, QueryResult, SavedConnection } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { DiffTablePicker } from './DiffTablePicker';

export type SourceMode = 'table' | 'query';

export interface DiffSourceState {
  mode: SourceMode;
  connectionId?: string;
  connection?: SavedConnection;
  sessionId?: string;
  namespaces?: Namespace[];
  namespace?: Namespace;
  tableName?: string;
  query?: string;
  result?: QueryResult;
  loading: boolean;
  connecting: boolean;
  namespacesLoading: boolean;
  error?: string;
  connectionError?: string;
}

interface DiffSourcePanelProps {
  label: string;
  connections: SavedConnection[];
  connectionsLoading?: boolean;
  source: DiffSourceState;
  onConnectionChange: (connectionId: string | null) => void;
  onNamespaceChange: (namespace: Namespace | null) => void;
  onSourceChange: (source: Partial<DiffSourceState>) => void;
  onExecute: () => void;
  disabled?: boolean;
}

function formatNamespace(namespace: Namespace): string {
  return namespace.schema ? `${namespace.database}.${namespace.schema}` : namespace.database;
}

function getNamespaceKey(namespace: Namespace): string {
  return `${namespace.database}:${namespace.schema ?? ''}`;
}

export function DiffSourcePanel({
  label,
  connections,
  connectionsLoading = false,
  source,
  onConnectionChange,
  onNamespaceChange,
  onSourceChange,
  onExecute,
  disabled = false,
}: DiffSourcePanelProps) {
  const { t } = useTranslation();

  const namespaceOptions = source.namespaces ?? [];
  const namespaceValue = source.namespace ? getNamespaceKey(source.namespace) : '';

  const tableDisabled =
    disabled ||
    !source.sessionId ||
    !source.namespace ||
    source.connecting ||
    source.namespacesLoading;

  const canExecuteQuery =
    source.sessionId && source.namespace && source.mode === 'query' && source.query?.trim();

  const rowCount = useMemo(
    () => (source.result?.rows ? source.result.rows.length : 0),
    [source.result]
  );

  return (
    <div className="flex flex-col gap-3 p-4 border border-border rounded-lg bg-card">
      {/* Header with label */}
      <div className="flex items-center justify-between">
        <span className="text-sm font-medium text-muted-foreground">{label}</span>
        <div className="flex items-center gap-2 text-xs">
          {source.loading && (
            <span className="flex items-center gap-1 text-muted-foreground">
              <Loader2 size={12} className="animate-spin" />
              {t('common.loading')}
            </span>
          )}
          {!source.loading && source.result && (
            <span className="flex items-center gap-1 text-success">
              <CheckCircle2 size={12} />
              {t('diff.rowCount', { count: rowCount })}
            </span>
          )}
        </div>
      </div>

      {/* Connection / namespace selection */}
      <div className="grid grid-cols-2 gap-2">
        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">{t('diff.connection')}</span>
          <Select
            value={source.connectionId ?? ''}
            onValueChange={value => onConnectionChange(value || null)}
            disabled={disabled || connectionsLoading || source.connecting}
          >
            <SelectTrigger className="h-9">
              <SelectValue placeholder={t('diff.selectConnection')} />
            </SelectTrigger>
            <SelectContent>
              {connections.map(conn => (
                <SelectItem key={conn.id} value={conn.id}>
                  <div className="flex items-center gap-2">
                    <Server size={12} className="text-muted-foreground" />
                    <span className="truncate max-w-40">{conn.name}</span>
                    {conn.environment !== 'development' && (
                      <span className="text-[10px] uppercase text-warning">{conn.environment}</span>
                    )}
                    {conn.read_only && (
                      <span className="text-[10px] uppercase text-muted-foreground">RO</span>
                    )}
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>

        <div className="flex flex-col gap-1">
          <span className="text-xs text-muted-foreground">{t('diff.namespace')}</span>
          <Select
            value={namespaceValue}
            onValueChange={value => {
              const selected = namespaceOptions.find(ns => getNamespaceKey(ns) === value);
              onNamespaceChange(selected ?? null);
            }}
            disabled={tableDisabled || namespaceOptions.length === 0}
          >
            <SelectTrigger className="h-9">
              <SelectValue
                placeholder={
                  source.namespacesLoading ? t('common.loading') : t('diff.selectNamespace')
                }
              />
            </SelectTrigger>
            <SelectContent>
              {namespaceOptions.map(ns => (
                <SelectItem key={getNamespaceKey(ns)} value={getNamespaceKey(ns)}>
                  <div className="flex items-center gap-2">
                    <Database size={12} className="text-muted-foreground" />
                    <span className="truncate max-w-44">{formatNamespace(ns)}</span>
                  </div>
                </SelectItem>
              ))}
            </SelectContent>
          </Select>
        </div>
      </div>

      {/* Mode tabs */}
      <div className="flex border border-border rounded-md overflow-hidden">
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className={cn(
            'h-9 flex-1 rounded-none flex items-center justify-center gap-2 px-3 py-2 text-sm transition-colors',
            source.mode === 'table'
              ? 'bg-accent text-accent-foreground'
              : 'hover:bg-muted/50 text-muted-foreground'
          )}
          onClick={() => onSourceChange({ mode: 'table', error: undefined })}
          disabled={disabled}
        >
          <Table2 size={14} />
          {t('diff.modeTable')}
        </Button>
        <Button
          type="button"
          variant="ghost"
          size="sm"
          className={cn(
            'h-9 flex-1 rounded-none flex items-center justify-center gap-2 px-3 py-2 text-sm transition-colors border-l border-border',
            source.mode === 'query'
              ? 'bg-accent text-accent-foreground'
              : 'hover:bg-muted/50 text-muted-foreground'
          )}
          onClick={() => onSourceChange({ mode: 'query', error: undefined })}
          disabled={disabled}
        >
          <Code size={14} />
          {t('diff.modeQuery')}
        </Button>
      </div>

      {/* Source selector based on mode */}
      {source.mode === 'table' ? (
        <DiffTablePicker
          sessionId={source.sessionId ?? ''}
          namespace={source.namespace ?? { database: '' }}
          value={source.tableName}
          onSelect={tableName =>
            onSourceChange({ tableName: tableName || undefined, error: undefined })
          }
          disabled={tableDisabled}
          placeholder={t('diff.selectTable')}
        />
      ) : (
        <>
          <Textarea
            value={source.query ?? ''}
            onChange={e => onSourceChange({ query: e.target.value, error: undefined })}
            placeholder={t('diff.queryPlaceholder')}
            className="font-mono text-sm min-h-[100px] resize-y"
            disabled={disabled || source.connecting}
          />
          <Button
            onClick={onExecute}
            disabled={disabled || !canExecuteQuery || source.loading}
            variant="secondary"
            size="sm"
            className="w-full"
          >
            {source.loading ? (
              <>
                <Loader2 size={14} className="mr-2 animate-spin" />
                {t('query.running')}
              </>
            ) : (
              <>
                <Play size={14} className="mr-2" />
                {t('query.run')}
              </>
            )}
          </Button>
        </>
      )}

      {/* Error display */}
      {(source.connectionError || source.error) && (
        <div className="flex items-start gap-2 text-sm text-destructive">
          <AlertCircle size={14} className="shrink-0 mt-0.5" />
          <span className="break-all">{source.connectionError || source.error}</span>
        </div>
      )}
    </div>
  );
}

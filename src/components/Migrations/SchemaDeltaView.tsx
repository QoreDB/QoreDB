// SPDX-License-Identifier: BUSL-1.1

import { useTranslation } from 'react-i18next';
import type {
  ColumnChange,
  ObjectChange,
  SchemaDelta,
  TableChange,
  TableStatus,
} from '@/lib/migrations/schemaCompare';
import { cn } from '@/lib/utils';

const STATUS_STYLES: Record<TableStatus, string> = {
  added: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400',
  removed: 'bg-red-500/15 text-red-700 dark:text-red-400',
  modified: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
};

// Column change kinds grouped by the visual treatment they get.
const ADD_KINDS = new Set<ColumnChange['kind']>(['added']);
const REMOVE_KINDS = new Set<ColumnChange['kind']>(['removed']);

function changeTone(kind: ColumnChange['kind']): string {
  if (ADD_KINDS.has(kind)) return 'text-emerald-600 dark:text-emerald-400';
  if (REMOVE_KINDS.has(kind)) return 'text-red-600 dark:text-red-400';
  return 'text-amber-600 dark:text-amber-400';
}

function changeSymbol(kind: ColumnChange['kind']): string {
  if (ADD_KINDS.has(kind)) return '+';
  if (REMOVE_KINDS.has(kind)) return '−';
  return '~';
}

function objectSymbol(kind: ObjectChange['kind']): string {
  return kind === 'added' ? '+' : '−';
}

function objectTone(kind: ObjectChange['kind']): string {
  return kind === 'added'
    ? 'text-emerald-600 dark:text-emerald-400'
    : 'text-red-600 dark:text-red-400';
}

export function SchemaDeltaView({
  delta,
  emptyMessage,
}: {
  delta: SchemaDelta;
  emptyMessage?: string;
}) {
  const { t } = useTranslation();

  if (!delta.hasChanges) {
    return (
      <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
        {emptyMessage ?? t('migrations.driftNone')}
      </div>
    );
  }

  const statusLabel: Record<TableStatus, string> = {
    added: t('migrations.tableAdded'),
    removed: t('migrations.tableRemoved'),
    modified: t('migrations.tableModified'),
  };

  return (
    <div className="flex flex-col gap-3">
      <div className="flex flex-wrap items-center gap-2 text-xs">
        <SummaryChip
          tone={STATUS_STYLES.added}
          label={t('migrations.summaryAdded', { count: delta.summary.added })}
        />
        <SummaryChip
          tone={STATUS_STYLES.removed}
          label={t('migrations.summaryRemoved', { count: delta.summary.removed })}
        />
        <SummaryChip
          tone={STATUS_STYLES.modified}
          label={t('migrations.summaryModified', { count: delta.summary.modified })}
        />
      </div>

      <div className="flex flex-col gap-2">
        {delta.changes.map(change => (
          <TableChangeRow
            key={change.key}
            change={change}
            statusLabel={statusLabel[change.status]}
            t={t}
          />
        ))}
      </div>
    </div>
  );
}

function SummaryChip({ tone, label }: { tone: string; label: string }) {
  return <span className={cn('rounded px-1.5 py-0.5 font-medium', tone)}>{label}</span>;
}

function TableChangeRow({
  change,
  statusLabel,
  t,
}: {
  change: TableChange;
  statusLabel: string;
  t: (key: string) => string;
}) {
  return (
    <div className="rounded-md border border-border/60 overflow-hidden">
      <div className="flex items-center gap-2 px-3 py-1.5 bg-muted/30">
        <span
          className={cn(
            'rounded px-1.5 py-0.5 text-[10px] font-medium',
            STATUS_STYLES[change.status]
          )}
        >
          {statusLabel}
        </span>
        <span className="font-mono text-xs truncate">{change.key}</span>
      </div>
      {change.status === 'modified' && (
        <div className="px-3 py-2 flex flex-col gap-1 text-xs font-mono">
          {change.columns.map(col => (
            <div key={`col-${col.kind}-${col.column}`} className={changeTone(col.kind)}>
              <span className="mr-1">{changeSymbol(col.kind)}</span>
              {col.column}
              {col.detail && <span className="text-muted-foreground"> — {col.detail}</span>}
            </div>
          ))}
          {change.indexes.map(idx => (
            <div key={`idx-${idx.kind}-${idx.name}`} className={objectTone(idx.kind)}>
              <span className="mr-1">{objectSymbol(idx.kind)}</span>
              <span className="text-muted-foreground mr-1">{t('migrations.indexLabel')}</span>
              {idx.name}
            </div>
          ))}
          {change.foreignKeys.map(fk => (
            <div key={`fk-${fk.kind}-${fk.name}`} className={objectTone(fk.kind)}>
              <span className="mr-1">{objectSymbol(fk.kind)}</span>
              <span className="text-muted-foreground mr-1">{t('migrations.fkLabel')}</span>
              {fk.name}
            </div>
          ))}
        </div>
      )}
    </div>
  );
}

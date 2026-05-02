// SPDX-License-Identifier: BUSL-1.1

import { RotateCcw, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import type { ChangelogEntry } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { formatTimestamp, OperationBadge } from './OperationBadge';

interface RowHistoryPanelProps {
  entries: ChangelogEntry[];
  onClose: () => void;
  onRollback: (entry: ChangelogEntry) => void;
}

export function RowHistoryPanel({ entries, onClose, onRollback }: RowHistoryPanelProps) {
  const { t } = useTranslation();

  if (entries.length === 0) {
    return (
      <div className="p-4 text-center text-muted-foreground text-sm">
        {t('timeTravel.rowHistory.noHistory')}
      </div>
    );
  }

  return (
    <div className="flex flex-col h-full">
      <div className="flex items-center justify-between px-3 py-2 border-b border-border">
        <h3 className="text-sm font-medium">{t('timeTravel.rowHistory.title')}</h3>
        <Button variant="ghost" size="icon" className="h-6 w-6" onClick={onClose}>
          <X size={14} />
        </Button>
      </div>
      <ScrollArea className="flex-1">
        <div className="p-2 space-y-2">
          {entries.map(entry => (
            <RowHistoryEntry key={entry.id} entry={entry} onRollback={onRollback} />
          ))}
        </div>
      </ScrollArea>
    </div>
  );
}

function RowHistoryEntry({
  entry,
  onRollback,
}: {
  entry: ChangelogEntry;
  onRollback: (entry: ChangelogEntry) => void;
}) {
  const { t } = useTranslation();

  return (
    <div className="border border-border rounded-md p-2 text-xs space-y-1">
      <div className="flex items-center justify-between">
        <div className="flex items-center gap-2">
          <OperationBadge operation={entry.operation} />
          <span className="text-muted-foreground">{formatTimestamp(entry.timestamp)}</span>
        </div>
        <Button variant="ghost" size="sm" className="h-6 text-xs" onClick={() => onRollback(entry)}>
          <RotateCcw size={12} className="mr-1" />
          {t('timeTravel.rollback.rollbackToPoint')}
        </Button>
      </div>

      {entry.changed_columns.length > 0 && (
        <div className="text-muted-foreground">
          {t('timeTravel.rowHistory.changedColumns')}: {entry.changed_columns.join(', ')}
        </div>
      )}

      <div className="grid grid-cols-2 gap-2 mt-1">
        {entry.before && (
          <ValueSnapshot
            label={t('timeTravel.rowHistory.before')}
            data={entry.before}
            changedColumns={entry.changed_columns}
            variant="removed"
          />
        )}
        {entry.after && (
          <ValueSnapshot
            label={t('timeTravel.rowHistory.after')}
            data={entry.after}
            changedColumns={entry.changed_columns}
            variant="added"
          />
        )}
      </div>
    </div>
  );
}

function ValueSnapshot({
  label,
  data,
  changedColumns,
  variant,
}: {
  label: string;
  data: Record<string, unknown>;
  changedColumns: string[];
  variant: 'added' | 'removed';
}) {
  const borderColor = variant === 'added' ? 'border-emerald-500/20' : 'border-red-500/20';
  const bgColor = variant === 'added' ? 'bg-emerald-500/5' : 'bg-red-500/5';
  const hlColor =
    variant === 'added' ? 'text-emerald-400 font-semibold' : 'text-red-400 font-semibold';

  return (
    <div>
      <div className="text-muted-foreground mb-0.5 font-medium">{label}</div>
      <div className={cn('rounded p-1 font-mono border', bgColor, borderColor)}>
        {Object.entries(data).map(([k, v]) => (
          <div key={k} className={cn(changedColumns.includes(k) && hlColor)}>
            {k}: {JSON.stringify(v)}
          </div>
        ))}
      </div>
    </div>
  );
}

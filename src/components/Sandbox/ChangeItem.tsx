// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';
import { Plus, Pencil, Trash2, Undo2, Clock, Eye, ChevronDown } from 'lucide-react';
import { SandboxChange, SandboxChangeType } from '@/lib/sandboxTypes';
import { Value } from '@/lib/tauri';
import { Button } from '@/components/ui/button';
import { cn } from '@/lib/utils';
import { getChangeDiff } from '@/lib/sandboxOverlay';

interface ChangeItemProps {
  change: SandboxChange;
  onUndo: (changeId: string) => void;
  compact?: boolean;
  expanded?: boolean;
  onToggleDetails?: (changeId: string) => void;
}

const CHANGE_TYPE_CONFIG: Record<
  SandboxChangeType,
  { icon: typeof Plus; color: string; bgColor: string; borderColor: string; label: string }
> = {
  insert: {
    icon: Plus,
    color: 'text-success',
    bgColor: 'bg-success/10',
    borderColor: 'border-success/20',
    label: 'INSERT',
  },
  update: {
    icon: Pencil,
    color: 'text-warning',
    bgColor: 'bg-warning/10',
    borderColor: 'border-warning/20',
    label: 'UPDATE',
  },
  delete: {
    icon: Trash2,
    color: 'text-error',
    bgColor: 'bg-error/15',
    borderColor: 'border-error/30',
    label: 'DELETE',
  },
};

function formatValue(value: Value): string {
  if (value === null) return 'NULL';
  if (typeof value === 'boolean') return value ? 'true' : 'false';
  if (typeof value === 'number') return String(value);
  if (typeof value === 'string') {
    if (value.length > 30) return `"${value.slice(0, 30)}..."`;
    return `"${value}"`;
  }
  if (typeof value === 'object') {
    const json = JSON.stringify(value);
    if (json.length > 30) return `${json.slice(0, 30)}...`;
    return json;
  }
  return String(value);
}

function formatTimestamp(timestamp: number): string {
  const diff = Date.now() - timestamp;
  const minutes = Math.floor(diff / 60000);
  const hours = Math.floor(diff / 3600000);

  if (minutes < 1) return 'just now';
  if (minutes < 60) return `${minutes}m ago`;
  if (hours < 24) return `${hours}h ago`;
  return new Date(timestamp).toLocaleDateString();
}

export function ChangeItem({
  change,
  onUndo,
  compact = false,
  expanded = false,
  onToggleDetails,
}: ChangeItemProps) {
  const { t } = useTranslation();
  const config = CHANGE_TYPE_CONFIG[change.type];
  const Icon = config.icon;

  // Get primary key display
  const pkDisplay = change.primaryKey?.columns
    ? Object.entries(change.primaryKey.columns)
        .map(([k, v]) => `${k}=${formatValue(v)}`)
        .join(', ')
    : null;

  // Get changed values display
  const changedValues = change.newValues
    ? Object.entries(change.newValues)
        .slice(0, 3)
        .map(([k, v]) => `${k}=${formatValue(v)}`)
    : [];

  const moreValues =
    change.newValues && Object.keys(change.newValues).length > 3
      ? Object.keys(change.newValues).length - 3
      : 0;

  if (compact) {
    return (
      <div
        className={cn(
          'flex flex-col gap-2 px-2 py-1.5 rounded text-xs border-l-2',
          config.bgColor,
          config.borderColor
        )}
      >
        <div className="flex items-center gap-2">
          <Icon size={12} className={config.color} />
          <span className={cn('font-medium', config.color)}>{config.label}</span>
          {pkDisplay && (
            <span className="text-muted-foreground font-mono truncate">{pkDisplay}</span>
          )}
          <div className="ml-auto flex items-center gap-1.5">
            {onToggleDetails && (
              <Button
                variant="ghost"
                size="icon"
                className="h-5 w-5 shrink-0"
                onClick={() => onToggleDetails(change.id)}
                title={t('sandbox.changes.viewDetails')}
              >
                {expanded ? <ChevronDown size={10} /> : <Eye size={10} />}
              </Button>
            )}
            <Button
              variant="ghost"
              size="icon"
              className="h-5 w-5 shrink-0"
              onClick={() => onUndo(change.id)}
              title={t('sandbox.changes.undo')}
            >
              <Undo2 size={10} />
            </Button>
          </div>
        </div>

        {expanded && (
          <div className="space-y-1 rounded bg-background/40 p-2">
            {getChangeDiff(change).map(diff => (
              <div key={diff.column} className="grid grid-cols-3 gap-2 text-[11px]">
                <span className="font-mono text-foreground/80 truncate">{diff.column}</span>
                <span className="font-mono text-muted-foreground truncate">
                  {formatValue(diff.oldValue)}
                </span>
                <span className="font-mono text-foreground truncate">
                  {formatValue(diff.newValue)}
                </span>
              </div>
            ))}
          </div>
        )}
      </div>
    );
  }

  return (
    <div
      className={cn(
        'rounded-md border-l-2 border overflow-hidden',
        config.bgColor,
        config.borderColor,
        'border-r-border/50 border-t-border/50 border-b-border/50'
      )}
    >
      {/* Header */}
      <div className="flex items-center gap-2 px-3 py-2 border-b border-border/30">
        <div className={cn('flex items-center justify-center w-6 h-6 rounded', config.bgColor)}>
          <Icon size={14} className={config.color} />
        </div>
        <div className="flex-1 min-w-0">
          <div className="flex items-center gap-2">
            <span className={cn('text-xs font-semibold', config.color)}>{config.label}</span>
            {pkDisplay && (
              <span className="text-xs text-muted-foreground font-mono truncate">
                ({pkDisplay})
              </span>
            )}
          </div>
        </div>
        <div className="flex items-center gap-1 text-muted-foreground">
          <Clock size={10} />
          <span className="text-[10px]">{formatTimestamp(change.timestamp)}</span>
        </div>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={() => onUndo(change.id)}
          title={t('sandbox.changes.undo')}
        >
          <Undo2 size={12} />
        </Button>
      </div>

      {/* Values */}
      {changedValues.length > 0 && (
        <div className="px-3 py-2 space-y-1">
          {changedValues.map((val, idx) => (
            <div key={idx} className="text-xs font-mono text-foreground/80 truncate">
              {val}
            </div>
          ))}
          {moreValues > 0 && (
            <div className="text-xs text-muted-foreground">
              +{moreValues} {t('sandbox.changes.moreFields')}
            </div>
          )}
        </div>
      )}

      {/* Delete shows old values */}
      {change.type === 'delete' && change.oldValues && (
        <div className="px-3 py-2 space-y-1">
          {Object.entries(change.oldValues)
            .slice(0, 3)
            .map(([k, v]) => (
              <div key={k} className="text-xs font-mono text-foreground/60 line-through truncate">
                {k}={formatValue(v)}
              </div>
            ))}
        </div>
      )}
    </div>
  );
}

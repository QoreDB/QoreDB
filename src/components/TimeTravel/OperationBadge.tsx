// SPDX-License-Identifier: BUSL-1.1

import { cn } from '@/lib/utils';

const COLORS: Record<string, string> = {
  insert: 'bg-emerald-500/15 text-emerald-400 border-emerald-500/30',
  update: 'bg-amber-500/15 text-amber-400 border-amber-500/30',
  delete: 'bg-red-500/15 text-red-400 border-red-500/30',
};

const LABELS: Record<string, string> = {
  insert: 'INSERT',
  update: 'UPDATE',
  delete: 'DELETE',
};

export function OperationBadge({ operation }: { operation: string }) {
  return (
    <span
      className={cn(
        'text-[10px] px-1.5 py-0.5 font-mono rounded border inline-block',
        COLORS[operation]
      )}
    >
      {LABELS[operation] ?? operation}
    </span>
  );
}

export function PrimaryKeyDisplay({ pk }: { pk: Record<string, unknown> | null }) {
  if (!pk) return <span className="text-muted-foreground">-</span>;
  const entries = Object.entries(pk);
  if (entries.length === 0) return <span className="text-muted-foreground">-</span>;

  return (
    <span className="font-mono text-xs text-muted-foreground">
      {entries.map(([k, v]) => `${k}=${String(v)}`).join(', ')}
    </span>
  );
}

export function formatTimestamp(ts: string): string {
  const d = new Date(ts);
  return d.toLocaleString(undefined, {
    month: 'short',
    day: 'numeric',
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
  });
}

// SPDX-License-Identifier: Apache-2.0

import type { Value } from '../../lib/tauri';

interface RowModalUpdatePreviewProps {
  changes: { key: string; previous: Value; next: Value }[];
  isEmpty: boolean;
  error: string | null;
  title: string;
  emptyLabel: string;
  formatValue: (value: Value) => string;
}

export function RowModalUpdatePreview({
  changes,
  isEmpty,
  error,
  title,
  emptyLabel,
  formatValue,
}: RowModalUpdatePreviewProps) {
  return (
    <div
      className="border rounded-md p-3 mb-4 bg-(--q-accent-soft)"
      style={{ borderColor: 'var(--q-accent)' }}
    >
      <div className="text-xs font-semibold uppercase tracking-wide text-(--q-accent)">{title}</div>
      {isEmpty ? (
        <div className="text-xs text-muted-foreground mt-2">{emptyLabel}</div>
      ) : (
        <div className="mt-2 space-y-1">
          {changes.map(item => (
            <div key={item.key} className="flex items-center justify-between text-xs gap-3">
              <span className="font-mono text-muted-foreground min-w-0">{item.key}</span>
              <span className="font-mono text-muted-foreground line-through truncate">
                {formatValue(item.previous)}
              </span>
              <span className="font-mono font-semibold truncate text-(--q-accent-strong)">
                {formatValue(item.next)}
              </span>
            </div>
          ))}
        </div>
      )}
      {error && <div className="text-xs text-error mt-2">{error}</div>}
    </div>
  );
}

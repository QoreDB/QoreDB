// SPDX-License-Identifier: Apache-2.0

import type { CellContribution } from '@/lib/plugins';

interface PluginCellContributionListProps {
  title: string;
  allColumnsLabel: string;
  items: CellContribution[];
}

export function PluginCellContributionList({
  title,
  allColumnsLabel,
  items,
}: PluginCellContributionListProps) {
  if (items.length === 0) return null;
  return (
    <section>
      <h4 className="mb-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground">
        {title}
      </h4>
      <ul className="space-y-1">
        {items.map(item => (
          <li
            key={item.id}
            className="flex items-center justify-between gap-2 rounded border border-border px-2 py-1 text-xs"
          >
            <span className="min-w-0 truncate font-medium text-foreground" title={item.description}>
              {item.label}
            </span>
            <span className="shrink-0 font-mono text-muted-foreground">
              {item.match?.columnType ?? item.match?.namePattern ?? allColumnsLabel}
            </span>
          </li>
        ))}
      </ul>
    </section>
  );
}

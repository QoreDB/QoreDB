// SPDX-License-Identifier: Apache-2.0

import { ChevronRight } from 'lucide-react';
import type { Namespace } from '@/lib/tauri';

interface ContentBreadcrumbProps {
  connectionName?: string;
  namespace: Namespace;
  tableName?: string;
  onNavigateToDatabase?: (namespace: Namespace) => void;
}

export function ContentBreadcrumb({
  connectionName,
  namespace,
  tableName,
  onNavigateToDatabase,
}: ContentBreadcrumbProps) {
  const segments: { label: string; onClick?: () => void }[] = [];

  if (connectionName) {
    segments.push({ label: connectionName });
  }

  if (namespace.database) {
    const dbClickHandler =
      tableName && onNavigateToDatabase
        ? () => onNavigateToDatabase(namespace)
        : undefined;
    segments.push({ label: namespace.database, onClick: dbClickHandler });
  }

  if (namespace.schema) {
    segments.push({ label: namespace.schema });
  }

  if (tableName) {
    segments.push({ label: tableName });
  }

  return (
    <nav className="flex items-center gap-1 text-xs text-muted-foreground">
      {segments.map((seg, i) => {
        const isLast = i === segments.length - 1;
        return (
          <span key={`${seg.label}-${i}`} className="flex items-center gap-1">
            {i > 0 && <ChevronRight size={12} className="shrink-0" />}
            {seg.onClick && !isLast ? (
              <button
                type="button"
                onClick={seg.onClick}
                className="hover:text-foreground transition-colors"
              >
                {seg.label}
              </button>
            ) : (
              <span className={isLast ? 'text-foreground font-medium' : ''}>
                {seg.label}
              </span>
            )}
          </span>
        );
      })}
    </nav>
  );
}

// SPDX-License-Identifier: BUSL-1.1

import { useTranslation } from 'react-i18next';
import type { FederationSource } from '../../lib/federation';

interface FederationSourcesPanelProps {
  sources: FederationSource[];
  onInsertAlias?: (alias: string) => void;
}

const DRIVER_LABELS: Record<string, string> = {
  postgres: 'PostgreSQL',
  mysql: 'MySQL',
  mongodb: 'MongoDB',
  sqlite: 'SQLite',
  redis: 'Redis',
};

export function FederationSourcesPanel({
  sources,
  onInsertAlias,
}: FederationSourcesPanelProps) {
  const { t } = useTranslation();

  if (sources.length < 2) return null;

  return (
    <div className="flex items-center gap-1.5 px-3 py-1 border-b border-[var(--q-border)] bg-[var(--q-bg-1)] text-xs text-[var(--q-text-2)]">
      <span className="font-medium text-[var(--q-text-1)] shrink-0">
        {t('federation.sources')}
      </span>
      <div className="flex items-center gap-1 overflow-x-auto">
        {sources.map((source) => (
          <button
            key={source.sessionId}
            type="button"
            className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded bg-[var(--q-bg-2)] hover:bg-[var(--q-bg-3)] transition-colors cursor-pointer whitespace-nowrap"
            onClick={() => onInsertAlias?.(source.alias)}
            title={`${t('federation.clickToInsert')} ${source.alias}\n${source.displayName} (${DRIVER_LABELS[source.driver] ?? source.driver})`}
          >
            <DriverDot driver={source.driver} />
            <span className="font-mono">{source.alias}</span>
            <span className="text-(--q-text-3)">
              {source.displayName}
            </span>
          </button>
        ))}
      </div>
    </div>
  );
}

function DriverDot({ driver }: { driver: string }) {
  const colors: Record<string, string> = {
    postgres: '#336791',
    mysql: '#00758F',
    mongodb: '#47A248',
    sqlite: '#003B57',
    redis: '#DC382D',
  };
  const color = colors[driver] ?? 'var(--q-text-2)';

  return (
    <span
      className="inline-block w-2 h-2 rounded-full shrink-0"
      style={{ backgroundColor: color }}
    />
  );
}

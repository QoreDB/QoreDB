// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { SqlPreview } from '@/components/Sandbox/SqlPreview';
import type { DdlWarning } from '@/lib/ddl';
import type { Driver } from '@/lib/drivers';
import { translateDdlWarning } from './translateDdlWarning';

interface CreateTablePreviewProps {
  sql: string;
  warnings: DdlWarning[];
  driver: Driver;
}

export function CreateTablePreview({ sql, warnings, driver }: CreateTablePreviewProps) {
  const { t } = useTranslation();

  return (
    <div className="flex flex-col gap-3 h-full min-h-75">
      {warnings.length > 0 && (
        <div className="rounded-md border border-amber-500/40 bg-amber-500/10 p-3">
          <div className="flex items-center gap-2 text-amber-600 dark:text-amber-400 text-sm font-medium">
            <AlertTriangle size={14} />
            <span>{t('createTable.warningsCount', { count: warnings.length })}</span>
          </div>
          <ul className="mt-1 list-disc list-inside text-xs text-amber-700 dark:text-amber-300 space-y-0.5">
            {warnings.map((w, i) => (
              // biome-ignore lint/suspicious/noArrayIndexKey: warnings are regenerated deterministically; positional index is stable
              <li key={`${w.code}-${i}`}>{translateDdlWarning(t, w)}</li>
            ))}
          </ul>
        </div>
      )}
      <div className="flex-1 min-h-50 rounded-md border bg-muted/20 overflow-hidden">
        {sql ? (
          <SqlPreview value={sql} dialect={driver} className="h-full" />
        ) : (
          <p className="p-3 text-sm text-muted-foreground">{t('createTable.previewPlaceholder')}</p>
        )}
      </div>
    </div>
  );
}

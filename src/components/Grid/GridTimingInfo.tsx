// SPDX-License-Identifier: Apache-2.0

import { useTranslation } from 'react-i18next';

interface GridTimingInfoProps {
  execTimeMs?: number;
  totalTimeMs?: number;
}

export function GridTimingInfo({ execTimeMs, totalTimeMs }: GridTimingInfoProps) {
  const { t } = useTranslation();

  if (typeof execTimeMs !== 'number') return null;

  return (
    <span className="flex items-center gap-2 text-xs text-muted-foreground">
      <span title={t('query.time.execTooltip')}>
        {t('query.time.exec')}:{' '}
        <span className="font-mono font-medium text-foreground">{execTimeMs.toFixed(2)}ms</span>
      </span>
      {totalTimeMs !== undefined && (
        <>
          <span className="text-border/50">|</span>
          <span title={t('query.time.totalTooltip')}>
            {t('query.time.total')}:{' '}
            <span className="font-mono font-bold text-foreground">{totalTimeMs.toFixed(2)}ms</span>
          </span>
        </>
      )}
    </span>
  );
}

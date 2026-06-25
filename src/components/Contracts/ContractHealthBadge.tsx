// SPDX-License-Identifier: BUSL-1.1

import { useTranslation } from 'react-i18next';
import type { ContractRun } from '@/lib/contracts';
import { cn } from '@/lib/utils';

export type HealthLevel = 'healthy' | 'warning' | 'failing' | 'unknown';

export function deriveHealth(run: ContractRun | null | undefined): HealthLevel {
  if (!run) return 'unknown';
  if (run.fail_count > 0 || run.error_count > 0) return 'failing';
  if (run.results.some(r => r.status === 'skipped')) return 'warning';
  return 'healthy';
}

interface Props {
  run?: ContractRun | null;
  level?: HealthLevel;
  className?: string;
  /** Adds the localized label next to the pill. */
  withLabel?: boolean;
}

const LEVEL_STYLES: Record<HealthLevel, string> = {
  healthy: 'bg-emerald-500/15 text-emerald-600 dark:text-emerald-400',
  warning: 'bg-amber-500/15 text-amber-600 dark:text-amber-400',
  failing: 'bg-red-500/15 text-red-600 dark:text-red-400',
  unknown: 'bg-muted text-muted-foreground',
};

const DOT_STYLES: Record<HealthLevel, string> = {
  healthy: 'bg-emerald-500',
  warning: 'bg-amber-500',
  failing: 'bg-red-500',
  unknown: 'bg-muted-foreground/40',
};

export function ContractHealthBadge({ run, level, className, withLabel = true }: Props) {
  const { t } = useTranslation();
  const resolved = level ?? deriveHealth(run);

  return (
    <span
      className={cn(
        'inline-flex items-center gap-1.5 rounded-full px-2 py-0.5 text-xs font-medium',
        LEVEL_STYLES[resolved],
        className
      )}
      role="status"
      aria-label={t(`contracts.health.${resolved}`)}
    >
      <span aria-hidden className={cn('h-1.5 w-1.5 rounded-full', DOT_STYLES[resolved])} />
      {withLabel && <span>{t(`contracts.health.${resolved}`)}</span>}
    </span>
  );
}

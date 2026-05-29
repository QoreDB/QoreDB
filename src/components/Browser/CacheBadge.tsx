// SPDX-License-Identifier: Apache-2.0

import { RefreshCw } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';

interface CacheBadgeProps {
  ageMs: number;
  onRefresh: () => void;
}

/** Formats a millisecond duration as a compact `5s` / `3min` label. */
function formatAge(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.round(seconds / 60)}min`;
}

/**
 * Discreet refresh control shown when table data is served from the query
 * cache. The cache state stays out of the way — it lives in the tooltip rather
 * than a persistent badge — and clicking re-fetches live data.
 */
export function CacheBadge({ ageMs, onRefresh }: CacheBadgeProps) {
  const { t } = useTranslation();
  const fetchedAt = useRef(Date.now() - ageMs);
  const [age, setAge] = useState(() => formatAge(ageMs));

  useEffect(() => {
    fetchedAt.current = Date.now() - ageMs;
    setAge(formatAge(ageMs));
  }, [ageMs]);

  const syncAge = () => setAge(formatAge(Date.now() - fetchedAt.current));

  return (
    <Tooltip content={t('cache.fromCache', { age })}>
      <button
        type="button"
        onClick={onRefresh}
        onMouseEnter={syncAge}
        onFocus={syncAge}
        aria-label={t('cache.refresh')}
        className="ml-auto flex items-center justify-center rounded-md p-1 text-accent transition-colors hover:bg-muted hover:text-foreground"
      >
        <RefreshCw size={14} />
      </button>
    </Tooltip>
  );
}

// SPDX-License-Identifier: Apache-2.0

import { RefreshCw, Zap } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';

interface CacheBadgeProps {
  /** Age of the cached entry, in milliseconds, at the time it was fetched. */
  ageMs: number;
  /** Re-fetches live data, bypassing the cache. */
  onRefresh: () => void;
}

/** Formats a millisecond duration as a compact `5s` / `3min` label. */
function formatAge(ms: number): string {
  const seconds = Math.max(0, Math.round(ms / 1000));
  if (seconds < 60) return `${seconds}s`;
  return `${Math.round(seconds / 60)}min`;
}

/**
 * Discreet pill shown when table data is served from the query cache, with a
 * button to re-fetch live data. The age ticks every second so it stays accurate.
 */
export function CacheBadge({ ageMs, onRefresh }: CacheBadgeProps) {
  const { t } = useTranslation();
  const insertedAtRef = useRef(Date.now() - ageMs);
  const [now, setNow] = useState(Date.now());

  // Re-anchor when a new fetch reports a different age.
  useEffect(() => {
    insertedAtRef.current = Date.now() - ageMs;
    setNow(Date.now());
  }, [ageMs]);

  useEffect(() => {
    const id = setInterval(() => setNow(Date.now()), 1000);
    return () => clearInterval(id);
  }, []);

  const age = formatAge(now - insertedAtRef.current);

  return (
    <div className="ml-auto flex items-center gap-1.5 text-xs text-muted-foreground">
      <span className="flex items-center gap-1 rounded-md bg-muted px-2 py-1">
        <Zap size={12} className="text-accent" />
        {t('cache.fromCache', { age })}
      </span>
      <button
        type="button"
        onClick={onRefresh}
        title={t('cache.refresh')}
        className="flex items-center justify-center rounded-md p-1 transition-colors hover:bg-muted hover:text-foreground"
      >
        <RefreshCw size={12} />
      </button>
    </div>
  );
}

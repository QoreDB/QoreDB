// SPDX-License-Identifier: BUSL-1.1

import { Circle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Tooltip } from '@/components/ui/tooltip';
import { getRecordingStatus, type RecordingStatus } from '@/lib/replay';
import { useLicense } from '@/providers/LicenseProvider';

/** Recording happens in other tabs, so the status bar polls rather than listens. */
const POLL_MS = 2000;

export function ReplayIndicator() {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const unlocked = isFeatureEnabled('query_replay');
  const [status, setStatus] = useState<RecordingStatus | null>(null);

  useEffect(() => {
    if (!unlocked) return;
    let cancelled = false;
    const poll = async () => {
      try {
        const next = await getRecordingStatus();
        if (!cancelled) setStatus(next);
      } catch {
        if (!cancelled) setStatus(null);
      }
    };
    void poll();
    const timer = window.setInterval(() => void poll(), POLL_MS);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
    };
  }, [unlocked]);

  if (!status) return null;

  return (
    <Tooltip
      content={`${status.name} · ${t('replay.recordedCount', { count: status.entry_count })}`}
    >
      <span className="flex items-center gap-1.5 rounded-full border border-[var(--color-error)]/30 bg-[var(--color-error)]/10 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide text-[var(--color-error)]">
        <Circle size={9} className="fill-current animate-pulse" />
        {t('replay.recording')}
        <span className="font-normal normal-case">{status.entry_count}</span>
      </span>
    </Tooltip>
  );
}

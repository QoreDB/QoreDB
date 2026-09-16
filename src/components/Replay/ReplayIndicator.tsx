// SPDX-License-Identifier: BUSL-1.1

import { Circle, Square, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Popover, PopoverContent, PopoverTrigger } from '@/components/ui/popover';
import { notify } from '@/lib/notify';
import {
  cancelRecording,
  getRecordingStatus,
  RECORDING_CHANGED_EVENT,
  type RecordingStatus,
  stopRecording,
} from '@/lib/replay';
import { useLicense } from '@/providers/LicenseProvider';

/** Recording happens in other tabs, so the status bar polls rather than listens. */
const POLL_MS = 2000;

export function ReplayIndicator() {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const unlocked = isFeatureEnabled('query_replay');
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [open, setOpen] = useState(false);

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

  const finish = async (action: 'stop' | 'cancel') => {
    setOpen(false);
    try {
      if (action === 'stop') {
        const summary = await stopRecording();
        notify.success(t('replay.recordingSaved', { name: summary.name }));
      } else {
        await cancelRecording();
      }
      setStatus(null);
      window.dispatchEvent(new CustomEvent(RECORDING_CHANGED_EVENT));
    } catch (err) {
      notify.error(t('replay.errors.stopRecording'), String(err));
    }
  };

  return (
    <Popover open={open} onOpenChange={setOpen}>
      <PopoverTrigger asChild>
        <button
          type="button"
          className="flex items-center gap-1.5 rounded-full border border-[var(--color-error)]/30 bg-[var(--color-error)]/10 px-2.5 py-1 text-[10px] font-bold uppercase tracking-wide text-[var(--color-error)] focus:outline-none focus-visible:ring-1 focus-visible:ring-ring"
        >
          <Circle size={9} className="fill-current animate-pulse" />
          {t('replay.recording')}
          <span className="font-normal normal-case">{status.entry_count}</span>
        </button>
      </PopoverTrigger>
      <PopoverContent align="end" className="w-64 space-y-2 p-3">
        <p className="text-sm font-medium truncate">{status.name}</p>
        <p className="text-xs text-muted-foreground">
          {t('replay.recordedCount', { count: status.entry_count })}
          {status.excluded_mutations > 0 &&
            ` · ${t('replay.composition.excludedMutations', { count: status.excluded_mutations })}`}
        </p>
        <div className="flex items-center gap-2">
          <Button
            size="sm"
            className="h-7 flex-1 gap-1.5 text-xs"
            onClick={() => void finish('stop')}
          >
            <Square size={11} />
            {t('replay.stopRecording')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            onClick={() => void finish('cancel')}
            aria-label={t('replay.cancelRecording')}
            title={t('replay.cancelRecording')}
          >
            <X size={13} />
          </Button>
        </div>
      </PopoverContent>
    </Popover>
  );
}

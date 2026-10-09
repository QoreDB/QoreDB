// SPDX-License-Identifier: BUSL-1.1

import { Circle, Square, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
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
import { getWorkspaceState, useWorkspaceStore } from '@/lib/stores/workspaceStore';
import { useLicense } from '@/providers/LicenseProvider';

/** Recording happens in other tabs, so the status bar polls rather than listens. */
const POLL_MS = 2000;

export function ReplayIndicator() {
  const projectId = useWorkspaceStore(state => state.projectId);
  return <WorkspaceReplayIndicator key={projectId} projectId={projectId} />;
}

function WorkspaceReplayIndicator({ projectId }: { projectId: string }) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const unlocked = isFeatureEnabled('query_replay');
  const [status, setStatus] = useState<RecordingStatus | null>(null);
  const [open, setOpen] = useState(false);
  const [busy, setBusy] = useState(false);
  const busyRef = useRef(false);
  const alive = useRef(true);
  const readRef = useRef(0);
  const isCurrent = useCallback(
    () => alive.current && getWorkspaceState().projectId === projectId,
    [projectId]
  );

  useEffect(() => {
    alive.current = true;
    return () => {
      alive.current = false;
    };
  }, []);

  useEffect(() => {
    if (!unlocked) return;
    let cancelled = false;
    const poll = async () => {
      if (busyRef.current || !isCurrent()) return;
      const read = ++readRef.current;
      try {
        const next = await getRecordingStatus(projectId);
        if (!cancelled && isCurrent() && read === readRef.current) setStatus(next);
      } catch {
        if (!cancelled && isCurrent() && read === readRef.current) setStatus(null);
      }
    };
    const changed = () => void poll();
    void poll();
    const timer = window.setInterval(changed, POLL_MS);
    window.addEventListener(RECORDING_CHANGED_EVENT, changed);
    return () => {
      cancelled = true;
      window.clearInterval(timer);
      window.removeEventListener(RECORDING_CHANGED_EVENT, changed);
    };
  }, [isCurrent, projectId, unlocked]);

  if (!unlocked || !status) return null;

  const finish = async (action: 'stop' | 'cancel') => {
    if (!isCurrent() || busyRef.current) return;
    busyRef.current = true;
    setBusy(true);
    ++readRef.current;
    setOpen(false);
    const target = { projectId, runId: status.run_id };
    try {
      if (action === 'stop') {
        const summary = await stopRecording(target);
        if (!isCurrent()) return;
        notify.success(t('replay.recordingSaved', { name: summary.name }));
      } else {
        await cancelRecording(target);
        if (!isCurrent()) return;
      }
      setStatus(null);
    } catch (err) {
      if (isCurrent()) notify.error(t('replay.errors.stopRecording'), String(err));
    } finally {
      if (isCurrent()) {
        busyRef.current = false;
        setBusy(false);
        window.dispatchEvent(new CustomEvent(RECORDING_CHANGED_EVENT));
      }
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
            disabled={busy}
            onClick={() => void finish('stop')}
          >
            <Square size={11} />
            {t('replay.stopRecording')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            className="h-7 w-7 p-0"
            disabled={busy}
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

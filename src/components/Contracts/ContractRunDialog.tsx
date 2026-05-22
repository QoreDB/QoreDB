// SPDX-License-Identifier: BUSL-1.1

import { Loader2, Play } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  type ContractRun,
  type ContractRunEvent,
  onContractRun,
  runContract,
} from '@/lib/contracts';

import { ContractResultsView } from './ContractResultsView';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Active session id to run against. */
  sessionId: string | null;
  /** Saved connection id (optional, informational). */
  connectionId?: string | null;
  /** Display name for the dialog title. */
  contractName: string;
  /** YAML source to run. */
  contractSource: string;
}

type Phase = 'idle' | 'running' | 'completed' | 'failed';

interface Progress {
  index: number;
  total: number;
  ruleId: string | null;
}

export function ContractRunDialog({
  open,
  onClose,
  sessionId,
  connectionId,
  contractName,
  contractSource,
}: Props) {
  const { t } = useTranslation();
  const [phase, setPhase] = useState<Phase>('idle');
  const [progress, setProgress] = useState<Progress>({ index: 0, total: 0, ruleId: null });
  const [run, setRun] = useState<ContractRun | null>(null);
  const [error, setError] = useState<string | null>(null);

  // Reset whenever the dialog re-opens for a new contract.
  // contractName is intentionally in the dep list so re-opening with a
  // different contract while the dialog is still mounted resets the state.
  // biome-ignore lint/correctness/useExhaustiveDependencies: see above
  useEffect(() => {
    if (open) {
      setPhase('idle');
      setProgress({ index: 0, total: 0, ruleId: null });
      setRun(null);
      setError(null);
    }
  }, [open, contractName]);

  const handleRun = useCallback(async () => {
    if (!sessionId) {
      toast.error(t('contracts.run.connectionMissing'));
      return;
    }
    setPhase('running');
    setRun(null);
    setError(null);
    let unlisten: (() => void) | null = null;
    try {
      unlisten = await onContractRun((event: ContractRunEvent) => {
        switch (event.type) {
          case 'started':
            setProgress({ index: 0, total: event.rules_total, ruleId: null });
            break;
          case 'rule_started':
            setProgress({ index: event.index, total: event.total, ruleId: event.rule_id });
            break;
          case 'progress':
            setProgress({ index: event.index, total: event.total, ruleId: event.result.id });
            break;
          case 'failed':
            setError(event.error);
            break;
          // 'completed' is also delivered through the awaited promise.
        }
      });
      const result = await runContract(sessionId, contractSource, connectionId ?? undefined);
      setRun(result);
      setPhase('completed');
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
      setPhase('failed');
      toast.error(t('contracts.errors.runFailed'), { description: msg });
    } finally {
      unlisten?.();
    }
  }, [sessionId, contractSource, connectionId, t]);

  const canRun = phase !== 'running' && Boolean(sessionId);

  return (
    <Dialog open={open} onOpenChange={v => !v && onClose()}>
      <DialogContent className="max-w-3xl">
        <DialogHeader>
          <DialogTitle>{t('contracts.run.dialogTitle', { name: contractName })}</DialogTitle>
          <DialogDescription>
            {sessionId ? t('contracts.description') : t('contracts.run.connectionMissing')}
          </DialogDescription>
        </DialogHeader>

        {phase === 'idle' && (
          <div className="py-6 text-sm text-muted-foreground">{t('contracts.description')}</div>
        )}

        {phase === 'running' && (
          <div className="py-6 flex flex-col items-center gap-3">
            <Loader2 className="animate-spin text-[var(--color-accent)]" size={28} />
            <div className="text-sm text-muted-foreground">
              {progress.total > 0
                ? t('contracts.run.ruleProgress', { index: progress.index, total: progress.total })
                : t('contracts.run.starting')}
            </div>
            {progress.total > 0 && (
              <div className="w-full max-w-md h-1.5 rounded-full bg-muted overflow-hidden">
                <div
                  className="h-full bg-[var(--color-accent)] transition-all"
                  style={{ width: `${(progress.index / progress.total) * 100}%` }}
                />
              </div>
            )}
          </div>
        )}

        {phase === 'failed' && error && (
          <div className="py-4 px-3 rounded-md border border-red-500/30 bg-red-500/10 text-sm text-red-700 dark:text-red-300">
            {error}
          </div>
        )}

        {phase === 'completed' && run && (
          <div className="max-h-[60vh] overflow-y-auto">
            <ContractResultsView run={run} />
          </div>
        )}

        <DialogFooter>
          <Button variant="ghost" onClick={onClose}>
            {t('contracts.close')}
          </Button>
          <Button onClick={handleRun} disabled={!canRun}>
            {phase === 'running' ? (
              <>
                <Loader2 className="animate-spin" />
                {t('contracts.running')}
              </>
            ) : phase === 'completed' ? (
              <>
                <Play />
                {t('contracts.rerun')}
              </>
            ) : (
              <>
                <Play />
                {t('contracts.run.action')}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

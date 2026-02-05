import { useTranslation } from 'react-i18next';
import { AlertTriangle, CheckCircle2, Loader2 } from 'lucide-react';

import { Button } from '@/components/ui/button';
import type { ExportProgress } from '@/lib/export';
import { cn } from '@/lib/utils';

interface ExportProgressToastProps {
  progress: ExportProgress;
  onCancel?: () => void;
}

function formatBytes(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  const kb = bytes / 1024;
  if (kb < 1024) return `${kb.toFixed(1)} KB`;
  const mb = kb / 1024;
  if (mb < 1024) return `${mb.toFixed(1)} MB`;
  const gb = mb / 1024;
  return `${gb.toFixed(2)} GB`;
}

function formatElapsed(ms: number): string {
  if (ms < 1000) return `${ms} ms`;
  const seconds = ms / 1000;
  if (seconds < 60) return `${seconds.toFixed(1)} s`;
  const minutes = Math.floor(seconds / 60);
  const remainder = Math.floor(seconds % 60);
  return `${minutes}m ${remainder}s`;
}

export function ExportProgressToast({ progress, onCancel }: ExportProgressToastProps) {
  const { t } = useTranslation();
  const isActive = progress.state === 'running' || progress.state === 'pending';
  const isSuccess = progress.state === 'completed';
  const isFailed = progress.state === 'failed';
  const speed =
    typeof progress.rows_per_second === 'number' && Number.isFinite(progress.rows_per_second)
      ? progress.rows_per_second
      : undefined;

  return (
    <div className="w-90 rounded-md border border-border bg-background p-3 shadow-lg">
      <div className="flex items-start justify-between gap-3">
        <div className="flex items-start gap-2">
          {isSuccess ? (
            <CheckCircle2 size={18} className="mt-0.5 text-emerald-500" />
          ) : isFailed ? (
            <AlertTriangle size={18} className="mt-0.5 text-destructive" />
          ) : (
            <Loader2 size={18} className="mt-0.5 animate-spin text-muted-foreground" />
          )}
          <div className="space-y-1">
            <p className="text-sm font-medium text-foreground">{t('export.toastTitle')}</p>
            <p className="text-xs text-muted-foreground">{t(`export.state.${progress.state}`)}</p>
          </div>
        </div>
        {isActive && onCancel ? (
          <Button variant="outline" size="sm" className="h-7 px-2 text-xs" onClick={onCancel}>
            {t('export.cancel')}
          </Button>
        ) : null}
      </div>

      <div className="mt-3 space-y-2">
        <div className="h-1.5 w-full rounded-full bg-muted">
          <div
            className={cn(
              'h-1.5 rounded-full transition-all',
              isActive && 'bg-accent/70 animate-pulse w-full',
              isSuccess && 'bg-emerald-500 w-full',
              progress.state === 'cancelled' && 'bg-muted-foreground w-full',
              isFailed && 'bg-destructive w-full'
            )}
          />
        </div>

        <div className="flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-muted-foreground">
          <span>{t('export.rows', { count: progress.rows_exported })}</span>
          <span>{t('export.bytes', { size: formatBytes(progress.bytes_written) })}</span>
          <span>{t('export.elapsed', { time: formatElapsed(progress.elapsed_ms) })}</span>
          {speed !== undefined && (
            <span>{t('export.speed', { speed: speed.toFixed(1) })}</span>
          )}
        </div>

        {progress.error ? <p className="text-xs text-destructive">{progress.error}</p> : null}
      </div>
    </div>
  );
}

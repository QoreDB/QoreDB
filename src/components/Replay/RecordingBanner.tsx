// SPDX-License-Identifier: BUSL-1.1

import { ChevronDown, ChevronRight, Circle, KeyRound, Square, Trash2, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { formatBytes, type RecordedPreview, type RecordingStatus } from '@/lib/replay';

interface RecordingBannerProps {
  recording: RecordingStatus;
  previews: RecordedPreview[];
  onStop: () => void;
  onCancel: () => void;
  onDiscard: (index: number) => void;
  onDiscardMutations: () => void;
}

export function RecordingBanner({
  recording,
  previews,
  onStop,
  onCancel,
  onDiscard,
  onDiscardMutations,
}: RecordingBannerProps) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);

  const reads = recording.entry_count - recording.mutation_count;
  const composition = [
    t('replay.composition.total', { count: recording.entry_count }),
    t('replay.composition.reads', { count: reads }),
    recording.mutation_count > 0 &&
      t('replay.composition.mutations', { count: recording.mutation_count }),
    recording.excluded_mutations > 0 &&
      t('replay.composition.excludedMutations', { count: recording.excluded_mutations }),
    recording.captured_bytes > 0 && formatBytes(recording.captured_bytes),
  ]
    .filter(Boolean)
    .join(' · ');

  return (
    <div className="shrink-0 border-b border-[var(--color-error)]/30 bg-[var(--color-error)]/5">
      <div className="flex items-center gap-2 px-4 py-2">
        <button
          type="button"
          onClick={() => setOpen(value => !value)}
          aria-expanded={open}
          className="flex min-w-0 flex-1 items-center gap-2 text-left focus:outline-none focus-visible:ring-1 focus-visible:ring-ring rounded-sm"
        >
          {open ? (
            <ChevronDown size={12} className="shrink-0 text-muted-foreground" />
          ) : (
            <ChevronRight size={12} className="shrink-0 text-muted-foreground" />
          )}
          <Circle
            size={9}
            className="shrink-0 fill-[var(--color-error)] text-[var(--color-error)] animate-pulse"
          />
          <span className="text-xs font-medium truncate">{recording.name}</span>
          <span className="text-xs text-muted-foreground truncate">{composition}</span>
        </button>

        {recording.mutation_count > 0 && (
          <Button
            variant="outline"
            size="sm"
            className="h-6 shrink-0 text-xs"
            onClick={onDiscardMutations}
          >
            {t('replay.removeMutations')}
          </Button>
        )}
        <Button size="sm" className="h-6 shrink-0 gap-1.5 text-xs" onClick={onStop}>
          <Square size={11} />
          {t('replay.stopRecording')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          className="h-6 w-6 shrink-0 p-0"
          onClick={onCancel}
          aria-label={t('replay.cancelRecording')}
          title={t('replay.cancelRecording')}
        >
          <X size={13} />
        </Button>
      </div>

      {open && (
        <div className="space-y-1 px-4 pb-2 pl-10">
          {recording.capture_stopped_reason && (
            <p className="text-[11px] text-muted-foreground">
              {t(`replay.captureStopped.${recording.capture_stopped_reason}`)}
            </p>
          )}
          {recording.ignored_other_session > 0 && (
            <p className="text-[11px] text-muted-foreground">
              {t('replay.ignoredOtherSession', { count: recording.ignored_other_session })}
            </p>
          )}
          {recording.secrets_detected > 0 && (
            <p className="flex items-start gap-1.5 text-[11px] text-[var(--color-warning)]">
              <KeyRound size={11} className="mt-0.5 shrink-0" />
              <span>
                {recording.secret_policy === 'redact'
                  ? t('replay.secretsRedacted', { count: recording.secrets_detected })
                  : t('replay.secretsDetected', { count: recording.secrets_detected })}
              </span>
            </p>
          )}
          {previews.length > 0 && (
            <ul className="max-h-48 overflow-y-auto space-y-0.5">
              {previews.map((preview, index) => (
                <li
                  key={`${preview.order}-${preview.query_preview}`}
                  className="group flex items-center gap-2 text-[11px]"
                >
                  <span className="w-5 shrink-0 text-right text-muted-foreground">
                    {preview.order}
                  </span>
                  <span className="font-mono truncate flex-1">{preview.query_preview}</span>
                  {preview.looks_like_secret && (
                    <KeyRound
                      size={11}
                      className="shrink-0 text-[var(--color-warning)]"
                      aria-label={t('replay.looksLikeSecret')}
                    />
                  )}
                  {preview.is_mutation && (
                    <span className="shrink-0 text-[var(--color-warning)]">
                      {t('replay.mutation')}
                    </span>
                  )}
                  <Button
                    variant="ghost"
                    size="sm"
                    className="h-5 w-5 shrink-0 p-0 opacity-0 group-hover:opacity-100 focus-visible:opacity-100"
                    onClick={() => onDiscard(index)}
                    aria-label={t('replay.discardEntry')}
                  >
                    <Trash2 size={11} />
                  </Button>
                </li>
              ))}
            </ul>
          )}
        </div>
      )}
    </div>
  );
}

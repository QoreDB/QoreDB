// SPDX-License-Identifier: BUSL-1.1

import { Circle, Square, Trash2, X } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import type { CaptureMode, RecordedPreview, RecordingStatus } from '@/lib/replay';
import { formatBytes } from '@/lib/replay';

interface RecordingPanelProps {
  recording: RecordingStatus | null;
  previews: RecordedPreview[];
  isProduction: boolean;
  onStart: (options: {
    name: string;
    ignoredColumns: string[];
    captureMode: CaptureMode;
    allowProductionCapture: boolean;
  }) => void;
  onStop: () => void;
  onCancel: () => void;
  onDiscard: (index: number) => void;
}

function parseColumns(raw: string): string[] {
  return raw
    .split(',')
    .map(column => column.trim())
    .filter(Boolean);
}

export function RecordingPanel({
  recording,
  previews,
  isProduction,
  onStart,
  onStop,
  onCancel,
  onDiscard,
}: RecordingPanelProps) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [ignored, setIgnored] = useState('updated_at');
  const [captureValues, setCaptureValues] = useState(true);
  const [allowProductionCapture, setAllowProductionCapture] = useState(false);

  if (recording) {
    return (
      <div className="rounded-md border border-border p-3 space-y-3">
        <div className="flex items-center justify-between gap-3">
          <div className="flex items-center gap-2 min-w-0">
            <Circle
              size={10}
              className="shrink-0 fill-[var(--color-error)] text-[var(--color-error)] animate-pulse"
            />
            <span className="text-sm font-medium truncate">{recording.name}</span>
            <span className="text-xs text-muted-foreground shrink-0">
              {t('replay.recordedCount', { count: recording.entry_count })}
              {recording.captured_bytes > 0 && ` · ${formatBytes(recording.captured_bytes)}`}
            </span>
          </div>
          <div className="flex items-center gap-2 shrink-0">
            <Button size="sm" className="h-7 gap-1.5" onClick={onStop}>
              <Square size={12} />
              {t('replay.stopRecording')}
            </Button>
            <Button variant="ghost" size="sm" className="h-7" onClick={onCancel}>
              <X size={14} />
            </Button>
          </div>
        </div>

        {recording.capture_stopped_reason && (
          <p className="text-xs text-muted-foreground">
            {t(`replay.captureStopped.${recording.capture_stopped_reason}`)}
          </p>
        )}

        {recording.ignored_other_session > 0 && (
          <p className="text-xs text-muted-foreground">
            {t('replay.ignoredOtherSession', { count: recording.ignored_other_session })}
          </p>
        )}

        {previews.length > 0 && (
          <ul className="max-h-48 overflow-y-auto space-y-1">
            {previews.map((preview, index) => (
              <li
                key={`${preview.order}-${preview.query_preview}`}
                className="flex items-center gap-2 text-xs group"
              >
                <span className="w-6 shrink-0 text-right text-muted-foreground">
                  {preview.order}
                </span>
                <span className="font-mono truncate flex-1">{preview.query_preview}</span>
                {preview.is_mutation && (
                  <span className="shrink-0 text-[var(--color-warning)]">
                    {t('replay.mutation')}
                  </span>
                )}
                <Button
                  variant="ghost"
                  size="sm"
                  className="h-5 w-5 p-0 opacity-0 group-hover:opacity-100"
                  onClick={() => onDiscard(index)}
                  aria-label={t('replay.discardEntry')}
                >
                  <Trash2 size={12} />
                </Button>
              </li>
            ))}
          </ul>
        )}
      </div>
    );
  }

  return (
    <div className="rounded-md border border-border p-3 space-y-3">
      <div className="space-y-1.5">
        <Label htmlFor="replay-name" className="text-xs">
          {t('replay.setName')}
        </Label>
        <Input
          id="replay-name"
          value={name}
          onChange={event => setName(event.target.value)}
          placeholder={t('replay.setNamePlaceholder')}
          className="h-7 text-xs"
        />
      </div>

      <div className="space-y-1.5">
        <Label htmlFor="replay-ignored" className="text-xs">
          {t('replay.ignoredColumns')}
        </Label>
        <Input
          id="replay-ignored"
          value={ignored}
          onChange={event => setIgnored(event.target.value)}
          placeholder="updated_at, last_seen_at"
          className="h-7 text-xs font-mono"
        />
        <p className="text-[11px] text-muted-foreground">{t('replay.ignoredColumnsHint')}</p>
      </div>

      <div className="flex items-center justify-between gap-3">
        <div className="min-w-0">
          <Label className="text-xs">{t('replay.captureValues')}</Label>
          <p className="text-[11px] text-muted-foreground">
            {captureValues ? t('replay.captureValuesOn') : t('replay.captureValuesOff')}
          </p>
        </div>
        <Switch checked={captureValues} onCheckedChange={setCaptureValues} />
      </div>

      {isProduction && captureValues && (
        <div className="flex items-center justify-between gap-3 rounded-md bg-[var(--color-bg-2)] p-2">
          <div className="min-w-0">
            <Label className="text-xs">{t('replay.productionCapture')}</Label>
            <p className="text-[11px] text-muted-foreground">{t('replay.productionCaptureHint')}</p>
          </div>
          <Switch checked={allowProductionCapture} onCheckedChange={setAllowProductionCapture} />
        </div>
      )}

      <p className="rounded-md bg-[var(--color-bg-2)] p-2 text-[11px] text-muted-foreground">
        {t('replay.queryTextWarning')}
      </p>

      <Button
        size="sm"
        className="h-7 w-full gap-1.5"
        disabled={!name.trim()}
        onClick={() =>
          onStart({
            name: name.trim(),
            ignoredColumns: parseColumns(ignored),
            captureMode: captureValues ? 'full' : 'metadata_only',
            allowProductionCapture,
          })
        }
      >
        <Circle size={10} className="fill-current" />
        {t('replay.startRecording')}
      </Button>
    </div>
  );
}

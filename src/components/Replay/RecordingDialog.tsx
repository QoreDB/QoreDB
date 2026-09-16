// SPDX-License-Identifier: BUSL-1.1

import { Circle } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { HelpIcon } from '@/components/ui/help-icon';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { Switch } from '@/components/ui/switch';
import type { CaptureMode } from '@/lib/replay';

export interface RecordingSetup {
  name: string;
  ignoredColumns: string[];
  recordMutations: boolean;
  captureMode: CaptureMode;
  allowProductionCapture: boolean;
}

interface RecordingDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  isProduction: boolean;
  onStart: (setup: RecordingSetup) => void;
}

function parseColumns(raw: string): string[] {
  return raw
    .split(',')
    .map(column => column.trim())
    .filter(Boolean);
}

export function RecordingDialog({
  open,
  onOpenChange,
  isProduction,
  onStart,
}: RecordingDialogProps) {
  const { t } = useTranslation();
  const [name, setName] = useState('');
  const [ignored, setIgnored] = useState('updated_at');
  const [captureValues, setCaptureValues] = useState(true);
  const [recordMutations, setRecordMutations] = useState(false);
  const [allowProductionCapture, setAllowProductionCapture] = useState(false);

  const start = () => {
    onStart({
      name: name.trim(),
      ignoredColumns: parseColumns(ignored),
      recordMutations,
      captureMode: captureValues ? 'full' : 'metadata_only',
      allowProductionCapture,
    });
    onOpenChange(false);
    setName('');
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('replay.newRecording')}</DialogTitle>
        </DialogHeader>

        <form
          className="space-y-4"
          onSubmit={event => {
            event.preventDefault();
            if (name.trim()) start();
          }}
        >
          <div className="space-y-1.5">
            <Label htmlFor="replay-name" className="text-xs">
              {t('replay.setName')}
            </Label>
            <Input
              id="replay-name"
              autoFocus
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

          <div className="flex items-center justify-between gap-3">
            <div className="min-w-0">
              <Label className="text-xs">{t('replay.recordMutations')}</Label>
              <p className="text-[11px] text-muted-foreground">{t('replay.recordMutationsHint')}</p>
            </div>
            <Switch checked={recordMutations} onCheckedChange={setRecordMutations} />
          </div>

          {isProduction && captureValues && (
            <div className="flex items-center justify-between gap-3 rounded-md bg-[var(--color-bg-2)] p-2">
              <div className="min-w-0">
                <Label className="text-xs">{t('replay.productionCapture')}</Label>
                <p className="text-[11px] text-muted-foreground">
                  {t('replay.productionCaptureHint')}
                </p>
              </div>
              <Switch
                checked={allowProductionCapture}
                onCheckedChange={setAllowProductionCapture}
              />
            </div>
          )}

          <p className="flex items-start gap-1.5 text-[11px] text-muted-foreground">
            <span>{t('replay.queryTextWarning')}</span>
            <HelpIcon content={<p className="text-xs">{t('replay.queryTextWarningDetail')}</p>} />
          </p>

          <DialogFooter>
            <Button type="button" variant="ghost" size="sm" onClick={() => onOpenChange(false)}>
              {t('common.cancel')}
            </Button>
            <Button type="submit" size="sm" className="gap-1.5" disabled={!name.trim()}>
              <Circle size={10} className="fill-current" />
              {t('replay.startRecording')}
            </Button>
          </DialogFooter>
        </form>
      </DialogContent>
    </Dialog>
  );
}

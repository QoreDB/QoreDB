// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, Clock, Scissors } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';

export type OverrideLimitsKind = 'truncated' | 'timeout';

interface OverrideLimitsDialogProps {
  open: boolean;
  kind: OverrideLimitsKind;
  onOpenChange: (open: boolean) => void;
  onConfirm: () => void;
}

export function OverrideLimitsDialog({
  open,
  kind,
  onOpenChange,
  onConfirm,
}: OverrideLimitsDialogProps) {
  const { t } = useTranslation();

  const title =
    kind === 'truncated'
      ? t('query.overrideLimits.titleRows')
      : t('query.overrideLimits.titleTimeout');
  const description =
    kind === 'truncated'
      ? t('query.overrideLimits.descriptionRows')
      : t('query.overrideLimits.descriptionTimeout');
  const Icon = kind === 'truncated' ? Scissors : Clock;

  const handleConfirm = () => {
    onConfirm();
    onOpenChange(false);
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Icon size={16} className="text-warning" />
            {title}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-3">
          <p className="text-sm text-muted-foreground">{description}</p>

          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-sm text-warning">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <span>{t('query.overrideLimits.warning')}</span>
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button variant="destructive" onClick={handleConfirm}>
            {t('query.overrideLimits.confirm')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

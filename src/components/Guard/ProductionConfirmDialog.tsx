// SPDX-License-Identifier: Apache-2.0

import { AlertCircle } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';

interface ProductionConfirmDialogProps {
  open: boolean;
  title: string;
  description?: string;
  confirmationLabel: string;
  confirmLabel: string;
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
}

export function ProductionConfirmDialog({
  open,
  title,
  description,
  confirmationLabel,
  confirmLabel,
  onConfirm,
  onOpenChange,
}: ProductionConfirmDialogProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState('');

  useEffect(() => {
    if (open) {
      setValue('');
    }
  }, [open]);

  const isMatch = value.trim() === confirmationLabel;

  function handleConfirm() {
    if (!isMatch) return;
    onConfirm();
    onOpenChange(false);
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{title}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-sm text-warning">
            <AlertCircle size={16} className="mt-0.5 shrink-0" />
            <span>{t('environment.prodWarning')}</span>
          </div>

          {description && <p className="text-sm text-muted-foreground">{description}</p>}

          <div className="space-y-2">
            <label className="text-sm font-medium">
              {t('environment.confirmMessage', { name: confirmationLabel })}
            </label>
            <Input
              value={value}
              onChange={event => setValue(event.target.value)}
              placeholder={confirmationLabel}
              onKeyDown={event => {
                if (event.key === 'Enter' && isMatch) {
                  event.preventDefault();
                  handleConfirm();
                }
              }}
            />
          </div>
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button variant="destructive" onClick={handleConfirm} disabled={!isMatch}>
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

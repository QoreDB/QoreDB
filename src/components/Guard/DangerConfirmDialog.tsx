// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, Loader2 } from 'lucide-react';
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

interface DangerConfirmDialogProps {
  open: boolean;
  title: string;
  description: string;
  /** If provided, user must type this to confirm (for production) */
  confirmationLabel?: string;
  /** Extra info shown in the warning box */
  warningInfo?: string;
  confirmLabel: string;
  loading?: boolean;
  onConfirm: () => void;
  onOpenChange: (open: boolean) => void;
}

/**
 * A destructive action confirmation dialog with danger styling.
 * For production environments, requires typing the confirmation label.
 */
export function DangerConfirmDialog({
  open,
  title,
  description,
  confirmationLabel,
  warningInfo,
  confirmLabel,
  loading = false,
  onConfirm,
  onOpenChange,
}: DangerConfirmDialogProps) {
  const { t } = useTranslation();
  const [value, setValue] = useState('');

  useEffect(() => {
    if (open) {
      setValue('');
    }
  }, [open]);

  const requiresTyping = !!confirmationLabel;
  const isMatch = !requiresTyping || value.trim() === confirmationLabel;

  function handleConfirm() {
    if (!isMatch || loading) return;
    onConfirm();
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-destructive">
            <AlertTriangle size={18} />
            {title}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {/* Danger warning box */}
          <div className="flex items-start gap-2 rounded-md border border-destructive/30 bg-destructive/10 p-3 text-sm text-destructive">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <div className="space-y-1">
              <span>{description}</span>
              {warningInfo && <p className="font-medium">{warningInfo}</p>}
            </div>
          </div>

          {/* Typing confirmation for production */}
          {requiresTyping && (
            <div className="space-y-2">
              <label className="text-sm font-medium">
                {t('tableMenu.typeToConfirm', { name: confirmationLabel })}
              </label>
              <Input
                value={value}
                onChange={event => setValue(event.target.value)}
                placeholder={confirmationLabel}
                className="font-mono"
                onKeyDown={event => {
                  if (event.key === 'Enter' && isMatch && !loading) {
                    event.preventDefault();
                    handleConfirm();
                  }
                }}
                disabled={loading}
              />
            </div>
          )}
        </div>

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)} disabled={loading}>
            {t('common.cancel')}
          </Button>
          <Button variant="destructive" onClick={handleConfirm} disabled={!isMatch || loading}>
            {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {confirmLabel}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

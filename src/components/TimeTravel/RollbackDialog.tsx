// SPDX-License-Identifier: BUSL-1.1

import { useTranslation } from 'react-i18next';
import { Copy, FileCode, AlertTriangle } from 'lucide-react';
import { Button } from '@/components/ui/button';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import type { RollbackSqlResponse } from '@/lib/tauri';

interface RollbackDialogProps {
  open: boolean;
  result: RollbackSqlResponse | null;
  onClose: () => void;
  onCopy: () => void;
  onOpenInQueryTab: () => void;
}

export function RollbackDialog({
  open,
  result,
  onClose,
  onCopy,
  onOpenInQueryTab,
}: RollbackDialogProps) {
  const { t } = useTranslation();
  if (!result) return null;

  return (
    <Dialog open={open} onOpenChange={(o) => !o && onClose()}>
      <DialogContent className="max-w-2xl max-h-[80vh] flex flex-col">
        <DialogHeader>
          <DialogTitle>{t('timeTravel.rollback.title')}</DialogTitle>
        </DialogHeader>

        <div className="text-sm text-muted-foreground">
          {t('timeTravel.rollback.statements', { count: result.statements_count })}
        </div>

        {result.warnings.length > 0 && (
          <div className="text-sm text-amber-400 space-y-0.5">
            <div className="flex items-center gap-1 font-medium">
              <AlertTriangle size={14} />
              {t('timeTravel.rollback.warnings', { count: result.warnings.length })}
            </div>
            {result.warnings.map((w, i) => (
              <div key={i} className="text-xs ml-5">
                {w}
              </div>
            ))}
          </div>
        )}

        <ScrollArea className="flex-1 max-h-96">
          <pre className="text-xs font-mono bg-muted/50 rounded-md p-3 whitespace-pre-wrap">
            {result.sql}
          </pre>
        </ScrollArea>

        <DialogFooter>
          <Button variant="outline" onClick={onCopy}>
            <Copy size={14} className="mr-2" />
            {t('timeTravel.rollback.copyToClipboard')}
          </Button>
          <Button onClick={onOpenInQueryTab}>
            <FileCode size={14} className="mr-2" />
            {t('timeTravel.rollback.openInQueryTab')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

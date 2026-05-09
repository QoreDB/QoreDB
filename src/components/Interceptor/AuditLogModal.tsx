// SPDX-License-Identifier: Apache-2.0

import { FileText } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { AuditLogPanel } from './AuditLogPanel';

interface AuditLogModalProps {
  isOpen: boolean;
  onClose: () => void;
}

export function AuditLogModal({ isOpen, onClose }: AuditLogModalProps) {
  const { t } = useTranslation();

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent
        disableExitAnimation
        className="max-w-4xl max-h-[85vh] h-[85vh] flex flex-col p-0 gap-0"
      >
        <DialogHeader className="px-4 py-3 border-b border-border">
          <DialogTitle className="flex items-center gap-2 text-base">
            <FileText size={18} />
            {t('interceptor.audit.title')}
          </DialogTitle>
        </DialogHeader>
        <div className="flex-1 min-h-0 overflow-hidden">
          <AuditLogPanel />
        </div>
      </DialogContent>
    </Dialog>
  );
}

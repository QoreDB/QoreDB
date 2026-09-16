// SPDX-License-Identifier: Apache-2.0

import { FileText } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { LicenseGate } from '@/components/License/LicenseGate';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import type { AuditLogTab } from '@/lib/stores/modalStore';
import { cn } from '@/lib/utils';
import { AuditLogPanel } from './AuditLogPanel';
import { ProfilingPanel } from './ProfilingPanel';
import { QueryTrendsPanel } from './QueryTrendsPanel';

interface AuditLogModalProps {
  isOpen: boolean;
  initialTab: AuditLogTab;
  onClose: () => void;
}

const TABS: AuditLogTab[] = ['audit', 'profiling', 'slow', 'trends'];

export function AuditLogModal({ isOpen, initialTab, onClose }: AuditLogModalProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<AuditLogTab>(initialTab);

  useEffect(() => {
    if (isOpen) setTab(initialTab);
  }, [isOpen, initialTab]);

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent disableExitAnimation className="max-w-5xl h-[85vh] flex flex-col p-0 gap-0">
        <DialogHeader className="flex-row items-end justify-between gap-6 space-y-0 px-4 pt-2.5 border-b border-border pr-12">
          <DialogTitle className="flex items-center gap-2 pb-2.5 text-sm font-semibold">
            <FileText size={16} className="text-muted-foreground" />
            {t('interceptor.audit.title')}
          </DialogTitle>
          <div className="flex gap-1">
            {TABS.map(id => (
              <button
                key={id}
                type="button"
                onClick={() => setTab(id)}
                className={cn(
                  'relative -mb-px px-3 py-2 text-xs font-medium transition-colors border-b-2',
                  tab === id
                    ? 'border-accent text-foreground'
                    : 'border-transparent text-muted-foreground hover:text-foreground'
                )}
              >
                {t(`interceptor.audit.tabs.${id}`)}
              </button>
            ))}
          </div>
        </DialogHeader>
        <div className="flex-1 min-h-0 overflow-hidden">
          {tab === 'audit' && <AuditLogPanel />}
          {(tab === 'profiling' || tab === 'slow') && (
            <LicenseGate feature="profiling">
              <ProfilingPanel view={tab === 'slow' ? 'slow' : 'overview'} />
            </LicenseGate>
          )}
          {tab === 'trends' && <QueryTrendsPanel />}
        </div>
      </DialogContent>
    </Dialog>
  );
}

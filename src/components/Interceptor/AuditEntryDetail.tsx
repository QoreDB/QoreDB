// SPDX-License-Identifier: Apache-2.0

import { Filter } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { EnvironmentChip } from '@/components/ui/environment-chip';
import { ScrollArea } from '@/components/ui/scroll-area';
import { type AuditLogEntry, formatExecutionTime } from '../../lib/tauri/interceptor';
import { AuditStatusChip } from './AuditStatusChip';

interface AuditEntryDetailProps {
  entry: AuditLogEntry;
  onClose: () => void;
  getSafetyRuleLabel?: (ruleId?: string | null) => string;
  onFilterByFingerprint?: (fingerprint: string) => void;
}

function Field({
  label,
  children,
  className,
}: {
  label: string;
  children: ReactNode;
  className?: string;
}) {
  return (
    <div className={className}>
      <p className="text-[11px] uppercase tracking-wider text-muted-foreground">{label}</p>
      <div className="mt-0.5 text-sm">{children}</div>
    </div>
  );
}

export function AuditEntryDetail({
  entry,
  onClose,
  getSafetyRuleLabel,
  onFilterByFingerprint,
}: AuditEntryDetailProps) {
  const { t } = useTranslation();
  const fingerprint = entry.fingerprint;

  return (
    <Dialog open onOpenChange={open => !open && onClose()}>
      <DialogContent
        disableExitAnimation
        className="max-w-2xl max-h-[80vh] flex flex-col p-0 gap-0"
      >
        <DialogHeader className="px-4 py-2.5 border-b border-border pr-12 space-y-0">
          <DialogTitle className="flex items-center gap-2 text-sm font-semibold">
            {t('interceptor.audit.detail.title')}
            <EnvironmentChip environment={entry.environment} />
            <AuditStatusChip entry={entry} />
          </DialogTitle>
        </DialogHeader>

        <ScrollArea className="flex-1 min-h-0">
          <div className="p-4 space-y-4">
            <pre className="p-3 rounded-md bg-muted font-mono text-xs whitespace-pre-wrap break-all">
              {entry.query}
            </pre>

            {entry.safety_rule && (
              <p className="text-sm text-warning">
                {t(
                  entry.blocked
                    ? 'interceptor.audit.detail.blockedBy'
                    : 'interceptor.audit.detail.flaggedBy'
                )}
                {' · '}
                {getSafetyRuleLabel?.(entry.safety_rule) ?? entry.safety_rule}
              </p>
            )}

            {entry.error && <p className="text-sm text-error break-words">{entry.error}</p>}

            <div className="grid grid-cols-2 gap-x-6 gap-y-3">
              <Field label={t('interceptor.audit.detail.timestamp')}>
                {new Date(entry.timestamp).toLocaleString()}
              </Field>
              <Field label={t('interceptor.audit.detail.executionTime')}>
                {formatExecutionTime(entry.execution_time_ms)}
              </Field>
              <Field label={t('interceptor.audit.detail.database')}>{entry.database || '—'}</Field>
              <Field label={t('interceptor.audit.detail.rowCount')}>{entry.row_count ?? '—'}</Field>
              <Field label={t('interceptor.audit.columns.operation')}>
                <span className="font-mono text-xs">
                  {entry.operation_type} · {entry.driver_id} · {entry.source}
                </span>
              </Field>
              <Field label={t('interceptor.audit.detail.sessionId')}>
                <span className="font-mono text-xs">{entry.session_id}</span>
              </Field>
              {fingerprint && (
                <Field label={t('interceptor.audit.detail.fingerprint')} className="col-span-2">
                  <div className="flex items-center gap-2">
                    <code className="font-mono text-xs">{fingerprint}</code>
                    {onFilterByFingerprint && (
                      <Button
                        variant="ghost"
                        size="sm"
                        className="h-6 px-2 text-xs"
                        onClick={() => {
                          onFilterByFingerprint(fingerprint);
                          onClose();
                        }}
                      >
                        <Filter size={12} className="mr-1" />
                        {t('interceptor.audit.detail.filterByFingerprint')}
                      </Button>
                    )}
                  </div>
                </Field>
              )}
            </div>
          </div>
        </ScrollArea>
      </DialogContent>
    </Dialog>
  );
}

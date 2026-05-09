// SPDX-License-Identifier: Apache-2.0

import { Filter, X } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '../ui/button';
import { Label } from '../ui/label';
import { ScrollArea } from '../ui/scroll-area';
import {
  type AuditLogEntry,
  type Environment,
  formatExecutionTime,
} from '../../lib/tauri/interceptor';

function getEnvironmentColor(env: Environment): string {
  switch (env) {
    case 'development':
      return 'bg-green-500/10 text-green-600';
    case 'staging':
      return 'bg-yellow-500/10 text-yellow-600';
    case 'production':
      return 'bg-red-500/10 text-red-600';
    default:
      return 'bg-muted text-muted-foreground';
  }
}

interface AuditEntryDetailProps {
  entry: AuditLogEntry;
  onClose: () => void;
  getSafetyRuleLabel?: (ruleId?: string | null) => string;
  onFilterByFingerprint?: (fingerprint: string) => void;
}

export function AuditEntryDetail({
  entry,
  onClose,
  getSafetyRuleLabel,
  onFilterByFingerprint,
}: AuditEntryDetailProps) {
  const { t } = useTranslation();

  return (
    <div className="fixed inset-0 z-50 flex items-center justify-center">
      <button
        type="button"
        aria-label={t('common.close')}
        className="absolute inset-0 bg-black/50 cursor-default"
        onClick={onClose}
      />

      <div className="relative bg-background rounded-lg shadow-xl border border-border w-full max-w-2xl mx-4 max-h-[80vh] overflow-hidden flex flex-col">
        <div className="flex items-center justify-between p-4 border-b border-border">
          <h3 className="font-semibold">{t('interceptor.audit.detail.title')}</h3>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded hover:bg-muted transition-colors"
          >
            <X className="w-5 h-5" />
          </button>
        </div>

        <ScrollArea className="flex-1 p-4">
          <div className="space-y-4">
            <div className="flex items-center gap-2 flex-wrap">
              <span
                className={`text-xs px-2 py-1 rounded font-medium ${getEnvironmentColor(entry.environment)}`}
              >
                {entry.environment.toUpperCase()}
              </span>
              <span className="text-xs px-2 py-1 rounded bg-muted">{entry.operation_type}</span>
              <span className="text-xs px-2 py-1 rounded bg-muted">{entry.driver_id}</span>
              <span
                className={`text-xs px-2 py-1 rounded ${
                  entry.blocked
                    ? 'bg-yellow-500/10 text-yellow-600'
                    : entry.success
                      ? 'bg-green-500/10 text-green-600'
                      : 'bg-red-500/10 text-red-600'
                }`}
              >
                {entry.blocked
                  ? t('interceptor.audit.status.blocked')
                  : entry.success
                    ? t('interceptor.audit.status.success')
                    : t('interceptor.audit.status.failed')}
              </span>
            </div>

            <div>
              <Label className="text-sm font-medium">{t('interceptor.audit.detail.query')}</Label>
              <pre className="mt-1 p-3 rounded bg-muted font-mono text-sm whitespace-pre-wrap break-all">
                {entry.query}
              </pre>
            </div>

            <div className="grid grid-cols-2 gap-4 text-sm">
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.timestamp')}
                </Label>
                <p>{new Date(entry.timestamp).toLocaleString()}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.sessionId')}
                </Label>
                <p className="font-mono text-xs">{entry.session_id}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.database')}
                </Label>
                <p>{entry.database || '-'}</p>
              </div>
              <div>
                <Label className="text-muted-foreground">
                  {t('interceptor.audit.detail.executionTime')}
                </Label>
                <p>{formatExecutionTime(entry.execution_time_ms)}</p>
              </div>
              {entry.row_count != null && (
                <div>
                  <Label className="text-muted-foreground">
                    {t('interceptor.audit.detail.rowCount')}
                  </Label>
                  <p>{entry.row_count}</p>
                </div>
              )}
              {entry.fingerprint && (
                <div>
                  <Label className="text-muted-foreground">
                    {t('interceptor.audit.detail.fingerprint')}
                  </Label>
                  <div className="flex items-center gap-2 mt-1">
                    <code className="font-mono text-xs px-2 py-0.5 rounded bg-muted">
                      {entry.fingerprint}
                    </code>
                    {onFilterByFingerprint &&
                      (() => {
                        const fingerprint = entry.fingerprint;
                        if (!fingerprint) return null;
                        return (
                          <Button
                            variant="ghost"
                            size="sm"
                            className="h-6 px-2 text-xs"
                            onClick={() => {
                              onFilterByFingerprint(fingerprint);
                              onClose();
                            }}
                          >
                            <Filter className="w-3 h-3 mr-1" />
                            {t('interceptor.audit.detail.filterByFingerprint')}
                          </Button>
                        );
                      })()}
                  </div>
                </div>
              )}
            </div>

            {entry.blocked && entry.safety_rule && (
              <div>
                <Label className="text-sm font-medium text-yellow-600">
                  {t('interceptor.audit.detail.blockedBy')}
                </Label>
                <p className="mt-1 text-sm text-yellow-600">
                  {getSafetyRuleLabel?.(entry.safety_rule) ?? entry.safety_rule}
                </p>
              </div>
            )}

            {entry.error && (
              <div>
                <Label className="text-sm font-medium text-red-600">
                  {t('interceptor.audit.detail.error')}
                </Label>
                <p className="mt-1 text-sm text-red-600">{entry.error}</p>
              </div>
            )}
          </div>
        </ScrollArea>

        <div className="flex justify-end p-4 border-t border-border">
          <Button variant="outline" onClick={onClose}>
            {t('common.close')}
          </Button>
        </div>
      </div>
    </div>
  );
}

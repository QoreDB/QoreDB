// SPDX-License-Identifier: Apache-2.0

import { useState, useCallback, useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Copy, Download, Play, AlertTriangle, CheckCircle2, Loader2, FileCode } from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogHeader,
  DialogTitle,
  DialogFooter,
} from '@/components/ui/dialog';
import { MigrationScript, ApplySandboxResult } from '@/lib/sandboxTypes';
import { Driver } from '@/lib/drivers';
import { Environment } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { toast } from 'sonner';
import { SqlPreview } from './SqlPreview';

interface MigrationPreviewProps {
  isOpen: boolean;
  onClose: () => void;
  script: MigrationScript | null;
  loading?: boolean;
  error?: string | null;
  environment?: Environment;
  dialect?: Driver;
  onApply?: () => Promise<ApplySandboxResult>;
}

export function MigrationPreview({
  isOpen,
  onClose,
  script,
  loading = false,
  error = null,
  environment = 'development',
  dialect = Driver.Postgres,
  onApply,
}: MigrationPreviewProps) {
  const { t } = useTranslation();
  const [isApplying, setIsApplying] = useState(false);
  const [applyResult, setApplyResult] = useState<ApplySandboxResult | null>(null);
  const [confirmProd, setConfirmProd] = useState(false);
  const [confirmInput, setConfirmInput] = useState('');

  // Reset state when dialog opens
  useEffect(() => {
    if (isOpen) {
      setApplyResult(null);
      setConfirmProd(false);
      setConfirmInput('');
    }
  }, [isOpen]);

  const handleCopy = useCallback(async () => {
    if (!script?.sql) return;
    try {
      await navigator.clipboard.writeText(script.sql);
      toast.success(t('sandbox.migration.copied'));
    } catch {
      toast.error(t('sandbox.migration.copyFailed'));
    }
  }, [script, t]);

  const handleDownload = useCallback(() => {
    if (!script?.sql) return;

    const blob = new Blob([script.sql], { type: 'text/plain' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `migration_${new Date().toISOString().slice(0, 10)}.sql`;
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
    toast.success(t('sandbox.migration.downloaded'));
  }, [script, t]);

  const handleApply = useCallback(async () => {
    if (!onApply) return;

    // Production requires confirmation
    if (environment === 'production' && !confirmProd) {
      setConfirmProd(true);
      return;
    }

    if (environment === 'production' && confirmInput !== 'APPLY') {
      toast.error(t('sandbox.migration.confirmRequired'));
      return;
    }

    setIsApplying(true);
    try {
      const result = await onApply();
      setApplyResult(result);
      if (result.success) {
        toast.success(t('sandbox.migration.applySuccess', { count: result.applied_count }));
      } else {
        toast.error(t('sandbox.migration.applyFailed'), {
          description: result.error,
        });
      }
    } catch (err) {
      console.error('Error applying migration:', err);
      toast.error(t('sandbox.migration.applyFailed'));
    } finally {
      setIsApplying(false);
    }
  }, [onApply, environment, confirmProd, confirmInput, t]);

  return (
    <Dialog open={isOpen} onOpenChange={onClose}>
      <DialogContent className="max-w-3xl max-h-[90vh] overflow-hidden flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <FileCode size={20} />
            {t('sandbox.migration.title')}
          </DialogTitle>
        </DialogHeader>

        <div className="flex-1 min-h-0 overflow-hidden">
          {loading ? (
            <div className="flex items-center justify-center h-40 gap-2 text-muted-foreground">
              <Loader2 size={20} className="animate-spin" />
              <span>{t('sandbox.migration.generating')}</span>
            </div>
          ) : error ? (
            <div className="flex items-center gap-3 p-4 rounded-md bg-error/10 border border-error/20 text-error">
              <AlertTriangle size={18} />
              <span className="text-sm">{error}</span>
            </div>
          ) : script ? (
            <div className="space-y-4 h-full flex flex-col">
              {/* Warnings */}
              {script.warnings.length > 0 && (
                <div className="space-y-2">
                  {script.warnings.map((warning, idx) => (
                    <div
                      key={idx}
                      className="flex items-start gap-2 p-3 rounded-md bg-warning/10 border border-warning/20 text-warning text-sm"
                    >
                      <AlertTriangle size={14} className="shrink-0 mt-0.5" />
                      <span>{warning}</span>
                    </div>
                  ))}
                </div>
              )}

              {/* Stats */}
              <div className="flex items-center gap-4 text-sm text-muted-foreground">
                <span>{t('sandbox.migration.statements', { count: script.statement_count })}</span>
                <span>•</span>
                <span>{script.sql.length.toLocaleString()} characters</span>
              </div>

              {/* SQL Code */}
              <div className="flex-1 border border-border rounded-md bg-muted/30 overflow-hidden">
                <SqlPreview value={script.sql} dialect={dialect} className="h-full" />
              </div>

              {/* Apply Result */}
              {applyResult && (
                <div
                  className={cn(
                    'flex items-center gap-2 p-3 rounded-md border text-sm',
                    applyResult.success
                      ? 'bg-success/10 border-success/20 text-success'
                      : 'bg-error/10 border-error/20 text-error'
                  )}
                >
                  {applyResult.success ? (
                    <>
                      <CheckCircle2 size={16} />
                      <span>
                        {t('sandbox.migration.applySuccess', {
                          count: applyResult.applied_count,
                        })}
                      </span>
                    </>
                  ) : (
                    <>
                      <AlertTriangle size={16} />
                      <span>{applyResult.error || t('sandbox.migration.applyFailed')}</span>
                    </>
                  )}
                </div>
              )}

              {/* Production Confirmation */}
              {confirmProd && environment === 'production' && (
                <div className="p-4 rounded-md bg-error/10 border border-error/20 space-y-3">
                  <div className="flex items-start gap-2 text-error">
                    <AlertTriangle size={16} className="shrink-0 mt-0.5" />
                    <span className="text-sm font-medium">
                      {t('sandbox.migration.prodWarning')}
                    </span>
                  </div>
                  <div className="space-y-2">
                    <label className="text-sm text-muted-foreground">
                      {t('sandbox.migration.confirmLabel')}
                    </label>
                    <input
                      type="text"
                      value={confirmInput}
                      onChange={e => setConfirmInput(e.target.value)}
                      placeholder="APPLY"
                      className="w-full px-3 py-2 text-sm border border-border rounded-md bg-background focus:outline-none focus:ring-2 focus:ring-accent"
                    />
                  </div>
                </div>
              )}
            </div>
          ) : (
            <div className="flex items-center justify-center h-40 text-muted-foreground">
              {t('sandbox.migration.noScript')}
            </div>
          )}
        </div>

        <DialogFooter className="shrink-0 gap-2">
          <Button variant="outline" onClick={handleCopy} disabled={!script || loading}>
            <Copy size={14} className="mr-1.5" />
            {t('sandbox.migration.copy')}
          </Button>
          <Button variant="outline" onClick={handleDownload} disabled={!script || loading}>
            <Download size={14} className="mr-1.5" />
            {t('sandbox.migration.download')}
          </Button>
          {onApply && (
            <Button
              onClick={applyResult?.success ? onClose : handleApply}
              disabled={!script || loading || isApplying}
              className={cn(
                environment === 'production' &&
                  !applyResult?.success &&
                  'bg-error hover:bg-error/90'
              )}
            >
              {isApplying ? (
                <>
                  <Loader2 size={14} className="mr-1.5 animate-spin" />
                  {t('sandbox.migration.applying')}
                </>
              ) : applyResult?.success ? (
                <>
                  <CheckCircle2 size={14} className="mr-1.5" />
                  {t('common.close')}
                </>
              ) : (
                <>
                  <Play size={14} className="mr-1.5" />
                  {t('sandbox.migration.apply')}
                </>
              )}
            </Button>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

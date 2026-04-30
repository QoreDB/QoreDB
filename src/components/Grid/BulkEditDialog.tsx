// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, FileEdit, Loader2, Play } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { LicenseBadge } from '@/components/License/LicenseBadge';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { SqlPreview } from '@/components/Sandbox/SqlPreview';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import {
  BULK_EDIT_CORE_LIMIT,
  type BulkEditOperation,
  type BulkEditPlan,
  buildBulkEditChanges,
  eligibleColumnsForBulkEdit,
  validateBulkEdit,
} from '@/lib/bulkEdit';
import { Driver } from '@/lib/connection/drivers';
import type { MigrationScript, SandboxChangeDto } from '@/lib/sandbox/sandboxTypes';
import type { Namespace, TableSchema, Value } from '@/lib/tauri';
import { applySandboxChanges, generateMigrationSql } from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';
import type { RowData } from './utils/dataGridUtils';

interface BulkEditDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  selectedRows: RowData[];
  tableSchema: TableSchema | null;
  primaryKey?: string[];
  namespace?: Namespace;
  tableName?: string;
  sessionId?: string;
  dialect?: Driver;
  sandboxMode?: boolean;
  onSandboxUpdate?: (
    primaryKey: Record<string, Value>,
    oldValues: Record<string, Value>,
    newValues: Record<string, Value>
  ) => void;
  onApplied?: () => void;
}

const PREVIEW_DEBOUNCE_MS = 200;

export function BulkEditDialog({
  open,
  onOpenChange,
  selectedRows,
  tableSchema,
  primaryKey,
  namespace,
  tableName,
  sessionId,
  dialect = Driver.Postgres,
  sandboxMode = false,
  onSandboxUpdate,
  onApplied,
}: BulkEditDialogProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const hasPro = isFeatureEnabled('bulk_edit_unlimited');

  const eligibleColumns = useMemo(() => eligibleColumnsForBulkEdit(tableSchema), [tableSchema]);

  const [column, setColumn] = useState<string>('');
  const [operation, setOperation] = useState<BulkEditOperation>('set_value');
  const [value, setValue] = useState('');
  const [script, setScript] = useState<MigrationScript | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);
  const [previewError, setPreviewError] = useState<string | null>(null);
  const [isApplying, setIsApplying] = useState(false);

  useEffect(() => {
    if (!open) return;
    setColumn(prev => (prev && eligibleColumns.includes(prev) ? prev : (eligibleColumns[0] ?? '')));
    setOperation('set_value');
    setValue('');
    setScript(null);
    setPreviewError(null);
  }, [open, eligibleColumns]);

  const plan: BulkEditPlan = useMemo(
    () => ({ column, operation, value }),
    [column, operation, value]
  );

  const validation = useMemo(
    () =>
      validateBulkEdit({
        plan,
        rowCount: selectedRows.length,
        tableSchema,
        primaryKey,
        hasPro,
      }),
    [plan, selectedRows.length, tableSchema, primaryKey, hasPro]
  );

  const requiresPro = validation.errors.includes('requiresPro');
  const blocking = validation.errors.filter(e => e !== 'requiresPro');
  const canBuild = blocking.length === 0;

  const dtos: SandboxChangeDto[] = useMemo(() => {
    if (!canBuild || !namespace || !tableName || !primaryKey) return [];
    return buildBulkEditChanges({
      plan,
      rows: selectedRows,
      namespace,
      tableName,
      primaryKey,
      tableSchema,
    });
  }, [canBuild, plan, selectedRows, namespace, tableName, primaryKey, tableSchema]);

  useEffect(() => {
    if (!open || sandboxMode) return;
    if (!sessionId || dtos.length === 0 || requiresPro) {
      setScript(null);
      return;
    }

    let cancelled = false;
    setPreviewLoading(true);
    setPreviewError(null);

    const handle = window.setTimeout(async () => {
      try {
        const res = await generateMigrationSql(sessionId, dtos);
        if (cancelled) return;
        if (res.success && res.script) {
          setScript(res.script);
        } else {
          setScript(null);
          setPreviewError(res.error ?? t('bulkEdit.previewError'));
        }
      } catch (err) {
        if (cancelled) return;
        setScript(null);
        setPreviewError(err instanceof Error ? err.message : String(err));
      } finally {
        if (!cancelled) setPreviewLoading(false);
      }
    }, PREVIEW_DEBOUNCE_MS);

    return () => {
      cancelled = true;
      window.clearTimeout(handle);
    };
  }, [open, sandboxMode, sessionId, dtos, requiresPro, t]);

  const handleApply = useCallback(async () => {
    if (requiresPro || !canBuild || dtos.length === 0) return;

    if (sandboxMode) {
      if (!onSandboxUpdate) {
        toast.error(t('bulkEdit.noSandboxHandler'));
        return;
      }
      for (const dto of dtos) {
        onSandboxUpdate(dto.primary_key?.columns ?? {}, dto.old_values ?? {}, dto.new_values ?? {});
      }
      AnalyticsService.capture('bulk_edit_applied', {
        driver: dialect,
        affected_count: dtos.length,
        via_sandbox: true,
      });
      toast.success(t('bulkEdit.queuedInSandbox', { count: dtos.length }));
      onOpenChange(false);
      return;
    }

    if (!sessionId) {
      toast.error(t('bulkEdit.noSession'));
      return;
    }

    setIsApplying(true);
    try {
      const res = await applySandboxChanges(sessionId, dtos, true);
      if (res.success) {
        AnalyticsService.capture('bulk_edit_applied', {
          driver: dialect,
          affected_count: res.applied_count,
          via_sandbox: false,
        });
        toast.success(t('bulkEdit.applySuccess', { count: res.applied_count }));
        onApplied?.();
        onOpenChange(false);
      } else {
        toast.error(t('bulkEdit.applyError'), {
          description: res.error ?? undefined,
        });
      }
    } catch (err) {
      toast.error(t('bulkEdit.applyError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setIsApplying(false);
    }
  }, [
    requiresPro,
    canBuild,
    dtos,
    sandboxMode,
    onSandboxUpdate,
    sessionId,
    dialect,
    onApplied,
    onOpenChange,
    t,
  ]);

  const previewSql = sandboxMode ? null : (script?.sql ?? null);

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-3xl max-h-[85vh] flex flex-col">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2 text-sm">
            <FileEdit className="h-4 w-4 text-muted-foreground" />
            {t('bulkEdit.title')}
          </DialogTitle>
          <DialogDescription className="text-xs">
            {t('bulkEdit.subtitle', { count: selectedRows.length })}
          </DialogDescription>
        </DialogHeader>

        <div className="grid grid-cols-[1fr_auto_1fr] gap-3 items-end">
          <div className="space-y-1.5">
            <Label htmlFor="bulk-edit-column" className="text-xs">
              {t('bulkEdit.column')}
            </Label>
            <Select
              value={column}
              onValueChange={setColumn}
              disabled={eligibleColumns.length === 0}
            >
              <SelectTrigger id="bulk-edit-column" className="h-9">
                <SelectValue placeholder={t('bulkEdit.columnPlaceholder')} />
              </SelectTrigger>
              <SelectContent>
                {eligibleColumns.map(name => (
                  <SelectItem key={name} value={name}>
                    {name}
                  </SelectItem>
                ))}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-1.5">
            <Label htmlFor="bulk-edit-op" className="text-xs">
              {t('bulkEdit.operation')}
            </Label>
            <Select value={operation} onValueChange={v => setOperation(v as BulkEditOperation)}>
              <SelectTrigger id="bulk-edit-op" className="h-9 w-40">
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="set_value">{t('bulkEdit.setValue')}</SelectItem>
                <SelectItem value="set_null">{t('bulkEdit.setNull')}</SelectItem>
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-1.5">
            <Label htmlFor="bulk-edit-value" className="text-xs">
              {t('bulkEdit.value')}
            </Label>
            <Input
              id="bulk-edit-value"
              value={value}
              onChange={e => setValue(e.target.value)}
              placeholder={operation === 'set_null' ? 'NULL' : t('bulkEdit.valuePlaceholder')}
              disabled={operation === 'set_null'}
              className="h-9 font-mono text-sm"
            />
          </div>
        </div>

        {requiresPro && (
          <div className="flex flex-col items-center gap-2 rounded-md border border-dashed border-accent/30 p-4 text-center">
            <LicenseBadge tier="pro" />
            <p className="text-sm text-muted-foreground">
              {t('bulkEdit.proRequired', { limit: BULK_EDIT_CORE_LIMIT })}
            </p>
          </div>
        )}

        {!requiresPro && blocking.length > 0 && (
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-warning text-sm">
            <AlertTriangle size={14} className="shrink-0 mt-0.5" />
            <ul className="list-disc list-inside space-y-0.5">
              {blocking.map(err => (
                <li key={err}>{t(`bulkEdit.errors.${err}`)}</li>
              ))}
            </ul>
          </div>
        )}

        {!requiresPro && canBuild && (
          <div className="flex-1 min-h-0 flex flex-col gap-2">
            <div className="text-xs text-muted-foreground flex items-center gap-2">
              {sandboxMode ? (
                <span>{t('bulkEdit.sandboxNote', { count: dtos.length })}</span>
              ) : (
                <>
                  <span>
                    {t('bulkEdit.previewLabel', {
                      count: script?.statement_count ?? dtos.length,
                    })}
                  </span>
                  {previewLoading && <Loader2 size={12} className="animate-spin" />}
                </>
              )}
            </div>
            {!sandboxMode && (
              <div className="flex-1 min-h-0 border border-border rounded-md bg-muted/30 overflow-hidden">
                {previewError ? (
                  <div className="p-3 text-sm text-destructive">{previewError}</div>
                ) : previewSql ? (
                  <SqlPreview value={previewSql} dialect={dialect} className="h-full" />
                ) : (
                  <div className="p-3 text-sm text-muted-foreground italic">
                    {previewLoading ? t('bulkEdit.generating') : t('bulkEdit.previewEmpty')}
                  </div>
                )}
              </div>
            )}
            {script && script.warnings.length > 0 && (
              <div className="space-y-1">
                {script.warnings.map(w => (
                  <div
                    key={w}
                    className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-2 text-warning text-xs"
                  >
                    <AlertTriangle size={12} className="shrink-0 mt-0.5" />
                    <span>{w}</span>
                  </div>
                ))}
              </div>
            )}
          </div>
        )}

        <DialogFooter className="shrink-0">
          <Button variant="ghost" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            onClick={handleApply}
            disabled={
              requiresPro ||
              !canBuild ||
              dtos.length === 0 ||
              isApplying ||
              (!sandboxMode && previewLoading)
            }
          >
            {isApplying ? (
              <>
                <Loader2 size={14} className="mr-1.5 animate-spin" />
                {t('bulkEdit.applying')}
              </>
            ) : (
              <>
                <Play size={14} className="mr-1.5" />
                {sandboxMode
                  ? t('bulkEdit.addToSandbox')
                  : t('bulkEdit.apply', { count: dtos.length })}
              </>
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

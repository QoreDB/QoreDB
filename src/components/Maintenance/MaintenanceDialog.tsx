// SPDX-License-Identifier: Apache-2.0

import {
  AlertTriangle,
  CheckCircle2,
  ChevronRight,
  Info,
  Loader2,
  TriangleAlert,
  Wrench,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import { notify } from '@/lib/notify';
import type {
  Environment,
  MaintenanceMessageLevel,
  MaintenanceOperationInfo,
  MaintenanceOperationType,
  MaintenanceOptions,
  MaintenanceResult,
  Namespace,
  TableIndex,
} from '@/lib/tauri';
import { listMaintenanceOperations, runMaintenance } from '@/lib/tauri';

interface MaintenanceDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  environment: Environment;
  readOnly: boolean;
  indexes?: TableIndex[];
}

const OPERATION_LABEL_MAP: Record<MaintenanceOperationType, string> = {
  vacuum: 'maintenance.operations.vacuum',
  analyze: 'maintenance.operations.analyze',
  reindex: 'maintenance.operations.reindex',
  optimize: 'maintenance.operations.optimize',
  repair: 'maintenance.operations.repair',
  check: 'maintenance.operations.check',
  cluster: 'maintenance.operations.cluster',
  rebuild_indexes: 'maintenance.operations.rebuildIndexes',
  update_statistics: 'maintenance.operations.updateStatistics',
  compact: 'maintenance.operations.compact',
  validate: 'maintenance.operations.validate',
  integrity_check: 'maintenance.operations.integrityCheck',
  change_engine: 'maintenance.operations.changeEngine',
};

const OPERATION_DESC_MAP: Record<MaintenanceOperationType, string> = {
  vacuum: 'maintenance.operations.vacuumDesc',
  analyze: 'maintenance.operations.analyzeDesc',
  reindex: 'maintenance.operations.reindexDesc',
  optimize: 'maintenance.operations.optimizeDesc',
  repair: 'maintenance.operations.repairDesc',
  check: 'maintenance.operations.checkDesc',
  cluster: 'maintenance.operations.clusterDesc',
  rebuild_indexes: 'maintenance.operations.rebuildIndexesDesc',
  update_statistics: 'maintenance.operations.updateStatisticsDesc',
  compact: 'maintenance.operations.compactDesc',
  validate: 'maintenance.operations.validateDesc',
  integrity_check: 'maintenance.operations.integrityCheckDesc',
  change_engine: 'maintenance.operations.changeEngineDesc',
};

function MessageLevelIcon({ level }: { level: MaintenanceMessageLevel }) {
  switch (level) {
    case 'error':
      return <AlertTriangle size={14} className="text-destructive shrink-0" />;
    case 'warning':
      return <TriangleAlert size={14} className="text-warning shrink-0" />;
    case 'status':
      return <CheckCircle2 size={14} className="text-success shrink-0" />;
    default:
      return <Info size={14} className="text-muted-foreground shrink-0" />;
  }
}

export function MaintenanceDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
  tableName,
  environment,
  readOnly,
  indexes,
}: MaintenanceDialogProps) {
  const { t } = useTranslation();
  const [operations, setOperations] = useState<MaintenanceOperationInfo[]>([]);
  const [selectedOp, setSelectedOp] = useState<MaintenanceOperationType | null>(null);
  const [options, setOptions] = useState<MaintenanceOptions>({});
  const [loading, setLoading] = useState(false);
  const [loadingOps, setLoadingOps] = useState(false);
  const [result, setResult] = useState<MaintenanceResult | null>(null);

  const selectedInfo = operations.find((op) => op.operation === selectedOp);
  const isProduction = environment === 'production';

  const loadOperations = useCallback(async () => {
    setLoadingOps(true);
    try {
      const res = await listMaintenanceOperations(
        sessionId,
        namespace.database,
        namespace.schema,
        tableName
      );
      if (res.success) {
        setOperations(res.operations);
        if (res.operations.length > 0) {
          setSelectedOp(res.operations[0].operation);
        }
      } else {
        notify.error(t('maintenance.error'), res.error);
      }
    } catch (err) {
      notify.error(t('maintenance.error'), err);
    } finally {
      setLoadingOps(false);
    }
  }, [sessionId, namespace, tableName, t]);

  useEffect(() => {
    if (open) {
      setResult(null);
      setOptions({});
      loadOperations();
    }
  }, [open, loadOperations]);

  // Reset options when changing operation
  // biome-ignore lint/correctness/useExhaustiveDependencies: intentional reset on selectedOp change
  useEffect(() => {
    setOptions({});
    setResult(null);
  }, [selectedOp]);

  async function handleExecute() {
    if (!selectedOp || loading || readOnly) return;

    setLoading(true);
    setResult(null);
    try {
      const res = await runMaintenance(
        sessionId,
        namespace.database,
        namespace.schema,
        tableName,
        { operation: selectedOp, options },
        selectedInfo?.is_heavy || isProduction
      );

      if (res.success && res.result) {
        setResult(res.result);
        if (res.result.success) {
          notify.success(t('maintenance.success'));
        } else {
          notify.warning(t('maintenance.error'));
        }
      } else {
        notify.error(t('maintenance.error'), res.error);
      }
    } catch (err) {
      notify.error(t('maintenance.error'), err);
    } finally {
      setLoading(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Wrench size={18} />
            {t('maintenance.title')} — {tableName}
          </DialogTitle>
        </DialogHeader>

        {readOnly && (
          <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-sm text-warning">
            <AlertTriangle size={16} className="mt-0.5 shrink-0" />
            <span>{t('maintenance.readOnlyBlocked')}</span>
          </div>
        )}

        {loadingOps ? (
          <div className="flex items-center justify-center py-8">
            <Loader2 className="h-6 w-6 animate-spin text-muted-foreground" />
          </div>
        ) : operations.length === 0 ? (
          <p className="py-4 text-center text-sm text-muted-foreground">
            {t('maintenance.noOperations')}
          </p>
        ) : (
          <div className="space-y-4">
            {/* Operation list */}
            <div className="space-y-1">
              <Label className="text-xs text-muted-foreground">
                {t('maintenance.selectOperation')}
              </Label>
              <div className="space-y-1">
                {operations.map((op) => (
                  <button
                    key={op.operation}
                    type="button"
                    className={`w-full rounded-md border px-3 py-2 text-left text-sm transition-colors ${
                      selectedOp === op.operation
                        ? 'border-accent bg-accent/10'
                        : 'border-transparent hover:bg-muted/50'
                    }`}
                    onClick={() => setSelectedOp(op.operation)}
                    disabled={readOnly}
                  >
                    <div className="flex items-center gap-2">
                      <ChevronRight
                        size={14}
                        className={`shrink-0 transition-transform ${
                          selectedOp === op.operation ? 'rotate-90' : ''
                        }`}
                      />
                      <span className="font-medium">
                        {t(OPERATION_LABEL_MAP[op.operation])}
                      </span>
                      {op.is_heavy && (
                        <span className="rounded-full bg-warning/15 px-2 py-0.5 text-[10px] font-medium text-warning">
                          {t('maintenance.heavyWarning').split('.')[0]}
                        </span>
                      )}
                    </div>
                    {selectedOp === op.operation && (
                      <p className="mt-1 pl-5 text-xs text-muted-foreground">
                        {t(OPERATION_DESC_MAP[op.operation])}
                      </p>
                    )}
                  </button>
                ))}
              </div>
            </div>

            {/* Options panel */}
            {selectedInfo?.has_options && selectedOp && (
              <div className="space-y-3 rounded-md border p-3">
                <Label className="text-xs text-muted-foreground">
                  {t('maintenance.options')}
                </Label>

                {/* PostgreSQL VACUUM options */}
                {selectedOp === 'vacuum' && (
                  <div className="space-y-2">
                    <div className="flex items-center gap-2">
                      <Checkbox
                        id="vacuum-full"
                        checked={options.full ?? false}
                        onCheckedChange={(checked) =>
                          setOptions((prev) => ({ ...prev, full: !!checked }))
                        }
                      />
                      <Label htmlFor="vacuum-full" className="text-sm">
                        {t('maintenance.optionLabels.full')}
                      </Label>
                    </div>
                    <div className="flex items-center gap-2">
                      <Checkbox
                        id="vacuum-analyze"
                        checked={options.with_analyze ?? false}
                        onCheckedChange={(checked) =>
                          setOptions((prev) => ({ ...prev, with_analyze: !!checked }))
                        }
                      />
                      <Label htmlFor="vacuum-analyze" className="text-sm">
                        {t('maintenance.optionLabels.withAnalyze')}
                      </Label>
                    </div>
                    <div className="flex items-center gap-2">
                      <Checkbox
                        id="vacuum-verbose"
                        checked={options.verbose ?? false}
                        onCheckedChange={(checked) =>
                          setOptions((prev) => ({ ...prev, verbose: !!checked }))
                        }
                      />
                      <Label htmlFor="vacuum-verbose" className="text-sm">
                        {t('maintenance.optionLabels.verbose')}
                      </Label>
                    </div>
                  </div>
                )}

                {/* PostgreSQL CLUSTER options */}
                {selectedOp === 'cluster' && indexes && indexes.length > 0 && (
                  <div className="space-y-2">
                    <Label htmlFor="cluster-index" className="text-sm">
                      {t('maintenance.optionLabels.indexName')}
                    </Label>
                    <select
                      id="cluster-index"
                      className="w-full rounded-md border bg-background px-3 py-2 text-sm"
                      value={options.index_name ?? ''}
                      onChange={(e) =>
                        setOptions((prev) => ({
                          ...prev,
                          index_name: e.target.value || undefined,
                        }))
                      }
                    >
                      <option value="">{t('maintenance.optionLabels.selectIndex')}</option>
                      {indexes.map((idx) => (
                        <option key={idx.name} value={idx.name}>
                          {idx.name} ({idx.columns.join(', ')})
                        </option>
                      ))}
                    </select>
                  </div>
                )}

                {/* MySQL Change Engine */}
                {selectedOp === 'change_engine' && (
                  <div className="space-y-2">
                    <Label htmlFor="target-engine" className="text-sm">
                      {t('maintenance.optionLabels.targetEngine')}
                    </Label>
                    <Input
                      id="target-engine"
                      placeholder={t('maintenance.optionLabels.selectEngine')}
                      value={options.target_engine ?? ''}
                      onChange={(e) =>
                        setOptions((prev) => ({
                          ...prev,
                          target_engine: e.target.value || undefined,
                        }))
                      }
                    />
                  </div>
                )}
              </div>
            )}

            {/* Heavy operation warning */}
            {selectedInfo?.is_heavy && (
              <div className="flex items-start gap-2 rounded-md border border-warning/30 bg-warning/10 p-3 text-xs text-warning">
                <AlertTriangle size={14} className="mt-0.5 shrink-0" />
                <span>{t('maintenance.heavyWarning')}</span>
              </div>
            )}

            {/* Results */}
            {result && (
              <div className="space-y-2 rounded-md border p-3">
                <Label className="text-xs text-muted-foreground">
                  {t('maintenance.results')}
                </Label>

                <div className="space-y-1">
                  <p className="text-xs text-muted-foreground">
                    {t('maintenance.executionTime', {
                      time: result.execution_time_ms.toFixed(1),
                    })}
                  </p>
                  <p className="text-xs text-muted-foreground">
                    {t('maintenance.executedCommand')}:{' '}
                    <code className="font-mono text-foreground">{result.executed_command}</code>
                  </p>
                </div>

                <ScrollArea className="max-h-32">
                  <div className="space-y-1">
                    {result.messages.map((msg, i) => (
                      <div key={i} className="flex items-start gap-2 text-xs">
                        <MessageLevelIcon level={msg.level} />
                        <span>{msg.text}</span>
                      </div>
                    ))}
                  </div>
                </ScrollArea>
              </div>
            )}
          </div>
        )}

        <DialogFooter>
          <Button variant="outline" onClick={() => onOpenChange(false)}>
            {t('common.close')}
          </Button>
          <Button
            onClick={handleExecute}
            disabled={!selectedOp || loading || readOnly || operations.length === 0}
          >
            {loading && <Loader2 className="mr-2 h-4 w-4 animate-spin" />}
            {loading ? t('maintenance.executing') : t('maintenance.execute')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

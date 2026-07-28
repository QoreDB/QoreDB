// SPDX-License-Identifier: Apache-2.0

import { Copy, Gauge, Trash2 } from 'lucide-react';
import { useSyncExternalStore } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import {
  getPaginationScopes,
  type PaginationScope,
  paginationReport,
  percentile,
  resetPaginationMetrics,
  subscribePaginationMetrics,
} from '@/lib/diagnostics/paginationMetrics';

interface PaginationMetricsPanelProps {
  isOpen: boolean;
  onClose: () => void;
}

function ms(value: number | null): string {
  return value === null ? '—' : `${Math.round(value)} ms`;
}

export function PaginationMetricsPanel({ isOpen, onClose }: PaginationMetricsPanelProps) {
  const { t } = useTranslation();
  const scopes = useSyncExternalStore(subscribePaginationMetrics, getPaginationScopes);

  async function handleCopy() {
    try {
      await navigator.clipboard.writeText(paginationReport());
      toast.success(t('paginationDiagnostics.copied'));
    } catch (err) {
      toast.error(t('paginationDiagnostics.copyError'), {
        description: err instanceof Error ? err.message : undefined,
      });
    }
  }

  const columns: Array<{ key: string; render: (scope: PaginationScope) => string }> = [
    { key: 'scope', render: scope => scope.label },
    { key: 'pages', render: scope => scope.pages.toLocaleString() },
    { key: 'rows', render: scope => scope.rows.toLocaleString() },
    { key: 'firstPage', render: scope => ms(scope.firstPageMs) },
    { key: 'p50', render: scope => ms(percentile(scope.pageMs, 50)) },
    { key: 'p95', render: scope => ms(percentile(scope.pageMs, 95)) },
    { key: 'firstSearch', render: scope => ms(scope.firstSearchMs) },
    {
      key: 'exactCounts',
      render: scope =>
        scope.exactCountsCancelled > 0
          ? `${scope.exactCounts} (${scope.exactCountsCancelled})`
          : String(scope.exactCounts),
    },
    { key: 'errors', render: scope => String(scope.errors) },
  ];

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent
        disableExitAnimation
        className="max-w-3xl max-h-[85vh] flex flex-col p-0 gap-0"
      >
        <DialogHeader className="px-4 py-3 border-b border-border">
          <DialogTitle className="flex items-center gap-2 text-base">
            <Gauge size={18} className="text-accent" />
            {t('paginationDiagnostics.title')}
          </DialogTitle>
        </DialogHeader>

        <p className="px-4 py-2 text-xs text-muted-foreground border-b border-border bg-muted/20">
          {t('paginationDiagnostics.description')}
        </p>

        <div className="flex-1 overflow-auto">
          {scopes.length === 0 ? (
            <p className="px-4 py-8 text-sm text-center text-muted-foreground">
              {t('paginationDiagnostics.empty')}
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead className="sticky top-0 bg-background border-b border-border">
                <tr>
                  {columns.map(column => (
                    <th key={column.key} className="px-3 py-2 text-left font-medium">
                      {t(`paginationDiagnostics.${column.key}`)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {scopes.map(scope => (
                  <tr key={scope.id} className="border-b border-border/50">
                    {columns.map(column => (
                      <td key={column.key} className="px-3 py-1.5 tabular-nums">
                        {column.render(scope)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>

        <div className="flex items-center justify-end gap-2 px-4 py-2 border-t border-border">
          <Button variant="ghost" size="sm" disabled={scopes.length === 0} onClick={handleCopy}>
            <Copy size={14} className="mr-1.5" />
            {t('paginationDiagnostics.copy')}
          </Button>
          <Button
            variant="ghost"
            size="sm"
            disabled={scopes.length === 0}
            onClick={resetPaginationMetrics}
          >
            <Trash2 size={14} className="mr-1.5" />
            {t('paginationDiagnostics.reset')}
          </Button>
        </div>
      </DialogContent>
    </Dialog>
  );
}

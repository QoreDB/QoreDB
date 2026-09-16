// SPDX-License-Identifier: Apache-2.0

import { Copy, Gauge, Info, Trash2 } from 'lucide-react';
import { useSyncExternalStore } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogHeader, DialogTitle } from '@/components/ui/dialog';
import { Tooltip } from '@/components/ui/tooltip';
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

interface Column {
  key: string;
  align: 'left' | 'right';
  render: (scope: PaginationScope) => string;
}

const COLUMNS: Column[] = [
  { key: 'scope', align: 'left', render: scope => scope.label },
  { key: 'pages', align: 'right', render: scope => scope.pages.toLocaleString() },
  { key: 'rows', align: 'right', render: scope => scope.rows.toLocaleString() },
  { key: 'firstPage', align: 'right', render: scope => ms(scope.firstPageMs) },
  { key: 'p50', align: 'right', render: scope => ms(percentile(scope.pageMs, 50)) },
  { key: 'p95', align: 'right', render: scope => ms(percentile(scope.pageMs, 95)) },
  { key: 'firstSearch', align: 'right', render: scope => ms(scope.firstSearchMs) },
  {
    key: 'exactCounts',
    align: 'right',
    render: scope =>
      scope.exactCountsCancelled > 0
        ? `${scope.exactCounts} (${scope.exactCountsCancelled})`
        : String(scope.exactCounts),
  },
  { key: 'errors', align: 'right', render: scope => String(scope.errors) },
];

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

  return (
    <Dialog open={isOpen} onOpenChange={open => !open && onClose()}>
      <DialogContent
        disableExitAnimation
        className="max-w-4xl max-h-[80vh] flex flex-col p-0 gap-0"
      >
        <DialogHeader className="flex-row items-center justify-between gap-3 space-y-0 px-4 py-2.5 border-b border-border pr-12">
          <DialogTitle className="flex items-center gap-2 text-sm font-semibold">
            <Gauge size={16} className="text-muted-foreground" />
            {t('paginationDiagnostics.title')}
            <Tooltip content={t('paginationDiagnostics.description')}>
              <Info size={13} className="text-muted-foreground/70" />
            </Tooltip>
          </DialogTitle>
          <div className="flex items-center gap-1">
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
        </DialogHeader>

        <div className="flex-1 min-h-0 overflow-auto">
          {scopes.length === 0 ? (
            <p className="px-4 py-12 text-sm text-center text-muted-foreground">
              {t('paginationDiagnostics.empty')}
            </p>
          ) : (
            <table className="w-full text-xs">
              <thead className="sticky top-0 bg-muted/40 text-[11px] uppercase tracking-wider text-muted-foreground">
                <tr>
                  {COLUMNS.map(column => (
                    <th
                      key={column.key}
                      className={`px-3 py-2 font-medium ${column.align === 'right' ? 'text-right' : 'text-left'}`}
                    >
                      {t(`paginationDiagnostics.${column.key}`)}
                    </th>
                  ))}
                </tr>
              </thead>
              <tbody>
                {scopes.map(scope => (
                  <tr key={scope.id} className="border-t border-border hover:bg-muted/30">
                    {COLUMNS.map(column => (
                      <td
                        key={column.key}
                        className={`px-3 py-1.5 tabular-nums ${
                          column.align === 'right' ? 'text-right' : 'text-left font-mono'
                        } ${column.key === 'errors' && scope.errors > 0 ? 'text-error' : ''}`}
                      >
                        {column.render(scope)}
                      </td>
                    ))}
                  </tr>
                ))}
              </tbody>
            </table>
          )}
        </div>
      </DialogContent>
    </Dialog>
  );
}

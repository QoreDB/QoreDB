// SPDX-License-Identifier: BUSL-1.1

/**
 * DiffToolbar - Toolbar with swap, export, and refresh actions
 */
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  GitCompare,
  ArrowLeftRight,
  Download,
  RefreshCw,
  FileJson,
  FileSpreadsheet,
  ChevronDown,
} from 'lucide-react';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { DiffResult, exportDiffAsCSV, exportDiffAsJSON } from '@/lib/diffUtils';
import { notify } from '@/lib/notify';

interface DiffToolbarProps {
  onSwap: () => void;
  onRefresh: () => void;
  diffResult: DiffResult | null;
  canSwap: boolean;
  canRefresh: boolean;
}

export function DiffToolbar({
  onSwap,
  onRefresh,
  diffResult,
  canSwap,
  canRefresh,
}: DiffToolbarProps) {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);

  const handleExportCSV = async () => {
    if (!diffResult) return;
    setExporting(true);
    try {
      const csv = exportDiffAsCSV(diffResult);
      await downloadFile(csv, 'diff-export.csv', 'text/csv');
      notify.success(t('diff.exportSuccess'));
    } catch (err) {
      notify.error(t('diff.exportError'), err);
    } finally {
      setExporting(false);
    }
  };

  const handleExportJSON = async () => {
    if (!diffResult) return;
    setExporting(true);
    try {
      const json = exportDiffAsJSON(diffResult);
      await downloadFile(json, 'diff-export.json', 'application/json');
      notify.success(t('diff.exportSuccess'));
    } catch (err) {
      notify.error(t('diff.exportError'), err);
    } finally {
      setExporting(false);
    }
  };

  const handleCopyCSV = async () => {
    if (!diffResult) return;
    try {
      const csv = exportDiffAsCSV(diffResult);
      await navigator.clipboard.writeText(csv);
      notify.success(t('common.copied'));
    } catch (err) {
      notify.error(t('diff.exportError'), err);
    }
  };

  const handleCopyJSON = async () => {
    if (!diffResult) return;
    try {
      const json = exportDiffAsJSON(diffResult);
      await navigator.clipboard.writeText(json);
      notify.success(t('common.copied'));
    } catch (err) {
      notify.error(t('diff.exportError'), err);
    }
  };

  return (
    <div className="flex items-center justify-between px-4 py-3 border-b border-border bg-muted/30">
      {/* Title */}
      <div className="flex items-center gap-2">
        <GitCompare size={20} className="text-muted-foreground" />
        <h2 className="font-medium">{t('diff.title')}</h2>
      </div>

      {/* Actions */}
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" onClick={onSwap} disabled={!canSwap}>
          <ArrowLeftRight size={16} className="mr-1.5" />
          {t('diff.swap')}
        </Button>

        <Button variant="ghost" size="sm" onClick={onRefresh} disabled={!canRefresh}>
          <RefreshCw size={16} className="mr-1.5" />
          {t('diff.refresh')}
        </Button>

        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <Button variant="outline" size="sm" disabled={!diffResult || exporting}>
              <Download size={16} className="mr-1.5" />
              {t('diff.export')}
              <ChevronDown size={14} className="ml-1" />
            </Button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="end">
            <DropdownMenuItem onClick={handleExportCSV}>
              <FileSpreadsheet size={14} className="mr-2" />
              {t('diff.exportCSV')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleExportJSON}>
              <FileJson size={14} className="mr-2" />
              {t('diff.exportJSON')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleCopyCSV}>
              <FileSpreadsheet size={14} className="mr-2" />
              {t('diff.copyCSV')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={handleCopyJSON}>
              <FileJson size={14} className="mr-2" />
              {t('diff.copyJSON')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>
    </div>
  );
}

/**
 * Download content as a file
 */
async function downloadFile(content: string, filename: string, mimeType: string) {
  const blob = new Blob([content], { type: mimeType });
  const url = URL.createObjectURL(blob);
  const a = document.createElement('a');
  a.href = url;
  a.download = filename;
  document.body.appendChild(a);
  a.click();
  document.body.removeChild(a);
  URL.revokeObjectURL(url);
}

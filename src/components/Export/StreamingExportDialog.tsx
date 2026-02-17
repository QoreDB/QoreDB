// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { save } from '@tauri-apps/plugin-dialog';
import { toast } from 'sonner';

import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { Checkbox } from '@/components/ui/checkbox';
import { useLicense } from '@/providers/LicenseProvider';
import type { ExportConfig, ExportFormat } from '@/lib/export';
import type { Namespace } from '@/lib/tauri';

interface StreamingExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  query: string;
  namespace?: Namespace;
  tableName?: string;
  onConfirm: (config: ExportConfig) => Promise<void>;
}

export function StreamingExportDialog({
  open,
  onOpenChange,
  query,
  namespace,
  tableName,
  onConfirm,
}: StreamingExportDialogProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const [format, setFormat] = useState<ExportFormat>('csv');
  const [outputPath, setOutputPath] = useState('');
  const [includeHeaders, setIncludeHeaders] = useState(true);
  const [sqlTableName, setSqlTableName] = useState('');
  const [batchSize, setBatchSize] = useState('1000');
  const [limit, setLimit] = useState('');
  const [submitting, setSubmitting] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  const extension = useMemo(() => {
    switch (format) {
      case 'json':
        return 'json';
      case 'sql_insert':
        return 'sql';
      case 'html':
        return 'html';
      case 'xlsx':
        return 'xlsx';
      case 'parquet':
        return 'parquet';
      default:
        return 'csv';
    }
  }, [format]);

  useEffect(() => {
    if (!open) return;
    setFormat('csv');
    setOutputPath('');
    setIncludeHeaders(true);
    setSqlTableName(tableName ?? '');
    setBatchSize('1000');
    setLimit('');
    setSubmitting(false);
    setShowAdvanced(false);
  }, [open, tableName]);

  const handleBrowse = async () => {
    const defaultName = (tableName || 'export').replace(/[\\/]/g, '_');
    const filePath = await save({
      defaultPath: `${defaultName}.${extension}`,
      filters: [
        {
          name: extension.toUpperCase(),
          extensions: [extension],
        },
      ],
    });

    if (filePath) {
      setOutputPath(filePath);
    }
  };

  const handleSubmit = async () => {
    if (!query.trim()) {
      toast.error(t('export.missingQuery'));
      return;
    }
    if (!outputPath) {
      toast.error(t('export.missingPath'));
      return;
    }
    if (format === 'sql_insert' && !sqlTableName.trim()) {
      toast.error(t('export.missingTable'));
      return;
    }

    const parsedBatch = Number(batchSize);
    const parsedLimit = Number(limit);

    const config: ExportConfig = {
      query,
      namespace,
      output_path: outputPath,
      format,
      table_name: format === 'sql_insert' ? sqlTableName.trim() : undefined,
      include_headers: format === 'csv' ? includeHeaders : true,
      batch_size: Number.isFinite(parsedBatch) && parsedBatch > 0 ? parsedBatch : undefined,
      limit: Number.isFinite(parsedLimit) && parsedLimit > 0 ? parsedLimit : undefined,
    };

    setSubmitting(true);
    try {
      await onConfirm(config);
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('export.dialogTitle')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="export-format">{t('export.formatLabel')}</Label>
            <Select value={format} onValueChange={value => setFormat(value as ExportFormat)}>
              <SelectTrigger id="export-format" className="w-full">
                <SelectValue placeholder={t('export.formatPlaceholder')} />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="csv">{t('export.format.csv')}</SelectItem>
                <SelectItem value="json">{t('export.format.json')}</SelectItem>
                <SelectItem value="sql_insert">{t('export.format.sql')}</SelectItem>
                <SelectItem value="html">{t('export.format.html')}</SelectItem>
                {isFeatureEnabled('export_xlsx') && (
                  <SelectItem value="xlsx">{t('export.format.xlsx')}</SelectItem>
                )}
                {isFeatureEnabled('export_parquet') && (
                  <SelectItem value="parquet">{t('export.format.parquet')}</SelectItem>
                )}
              </SelectContent>
            </Select>
          </div>

          <div className="space-y-2">
            <Label htmlFor="export-output">{t('export.outputLabel')}</Label>
            <div className="flex gap-2">
              <Input
                id="export-output"
                value={outputPath}
                onChange={event => setOutputPath(event.target.value)}
                placeholder={t('export.outputPlaceholder')}
              />
              <Button variant="outline" type="button" onClick={handleBrowse}>
                {t('export.browse')}
              </Button>
            </div>
          </div>

          {format === 'csv' && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="export-headers"
                checked={includeHeaders}
                onCheckedChange={checked => setIncludeHeaders(Boolean(checked))}
              />
              <Label htmlFor="export-headers">{t('export.includeHeaders')}</Label>
            </div>
          )}

          {format === 'sql_insert' && (
            <div className="space-y-2">
              <Label htmlFor="export-table">{t('export.tableLabel')}</Label>
              <Input
                id="export-table"
                value={sqlTableName}
                onChange={event => setSqlTableName(event.target.value)}
                placeholder={t('export.tablePlaceholder')}
              />
            </div>
          )}

          <div>
            <Button
              variant="ghost"
              type="button"
              className="px-0 text-xs text-primary/30"
              onClick={() => setShowAdvanced(prev => !prev)}
            >
              {showAdvanced ? t('export.hideAdvanced') : t('export.showAdvanced')}
            </Button>

            {showAdvanced && (
              <div className="mt-3 grid grid-cols-2 gap-3">
                <div className="space-y-2">
                  <Label htmlFor="export-batch">{t('export.batchLabel')}</Label>
                  <Input
                    id="export-batch"
                    type="number"
                    min={1}
                    value={batchSize}
                    onChange={event => setBatchSize(event.target.value)}
                  />
                </div>
                <div className="space-y-2">
                  <Label htmlFor="export-limit">{t('export.limitLabel')}</Label>
                  <Input
                    id="export-limit"
                    type="number"
                    min={1}
                    value={limit}
                    onChange={event => setLimit(event.target.value)}
                    placeholder={t('export.limitPlaceholder')}
                  />
                </div>
              </div>
            )}
          </div>
        </div>

        <DialogFooter className="gap-2">
          <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button type="button" onClick={handleSubmit} disabled={submitting}>
            {submitting ? t('export.starting') : t('export.start')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

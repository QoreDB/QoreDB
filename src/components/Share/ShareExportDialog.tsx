// SPDX-License-Identifier: Apache-2.0

import { useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { ExportFormat } from '@/lib/export';
import { useLicense } from '@/providers/LicenseProvider';

export interface ShareExportDialogRequest {
  file_name: string;
  format: ExportFormat;
  include_headers: boolean;
  table_name?: string;
  batch_size?: number;
  limit?: number;
}

interface ShareExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  defaultFileName: string;
  defaultTableName?: string;
  showBatchSize?: boolean;
  onConfirm: (request: ShareExportDialogRequest) => Promise<void>;
}

export function ShareExportDialog({
  open,
  onOpenChange,
  defaultFileName,
  defaultTableName,
  showBatchSize = true,
  onConfirm,
}: ShareExportDialogProps) {
  const { t } = useTranslation();
  const { isFeatureEnabled } = useLicense();
  const [fileName, setFileName] = useState(defaultFileName);
  const [format, setFormat] = useState<ExportFormat>('html');
  const [includeHeaders, setIncludeHeaders] = useState(true);
  const [sqlTableName, setSqlTableName] = useState(defaultTableName ?? '');
  const [batchSize, setBatchSize] = useState('1000');
  const [limit, setLimit] = useState('');
  const [acknowledged, setAcknowledged] = useState(false);
  const [submitting, setSubmitting] = useState(false);
  const [showAdvanced, setShowAdvanced] = useState(false);

  useEffect(() => {
    if (!open) return;
    setFileName(defaultFileName);
    setFormat('html');
    setIncludeHeaders(true);
    setSqlTableName(defaultTableName ?? '');
    setBatchSize('1000');
    setLimit('');
    setAcknowledged(false);
    setSubmitting(false);
    setShowAdvanced(false);
  }, [defaultFileName, defaultTableName, open]);

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

  async function handleSubmit() {
    if (!fileName.trim()) {
      toast.error(t('share.fileNameRequired'));
      return;
    }

    if (format === 'sql_insert' && !sqlTableName.trim()) {
      toast.error(t('export.missingTable'));
      return;
    }

    if (!acknowledged) {
      toast.error(t('share.confirmRequired'));
      return;
    }

    const parsedBatch = Number(batchSize);
    const parsedLimit = Number(limit);

    setSubmitting(true);
    try {
      await onConfirm({
        file_name: fileName.trim(),
        format,
        include_headers: format === 'csv' ? includeHeaders : true,
        table_name: format === 'sql_insert' ? sqlTableName.trim() : undefined,
        batch_size:
          showBatchSize && Number.isFinite(parsedBatch) && parsedBatch > 0
            ? parsedBatch
            : undefined,
        limit: Number.isFinite(parsedLimit) && parsedLimit > 0 ? parsedLimit : undefined,
      });
    } finally {
      setSubmitting(false);
    }
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-lg">
        <DialogHeader>
          <DialogTitle>{t('share.dialogTitle')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <div className="space-y-2">
            <Label htmlFor="share-file-name">{t('share.fileName')}</Label>
            <div className="flex items-center gap-2">
              <Input
                id="share-file-name"
                value={fileName}
                onChange={event => setFileName(event.target.value)}
                placeholder={t('share.fileNamePlaceholder')}
              />
              <span className="shrink-0 rounded-md border border-border bg-muted/30 px-2 py-1 text-xs text-muted-foreground">
                .{extension}
              </span>
            </div>
          </div>

          <div className="space-y-2">
            <Label htmlFor="share-format">{t('export.formatLabel')}</Label>
            <Select value={format} onValueChange={value => setFormat(value as ExportFormat)}>
              <SelectTrigger id="share-format" className="w-full">
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

          {format === 'csv' && (
            <div className="flex items-center gap-2">
              <Checkbox
                id="share-headers"
                checked={includeHeaders}
                onCheckedChange={checked => setIncludeHeaders(Boolean(checked))}
              />
              <Label htmlFor="share-headers">{t('export.includeHeaders')}</Label>
            </div>
          )}

          {format === 'sql_insert' && (
            <div className="space-y-2">
              <Label htmlFor="share-table">{t('export.tableLabel')}</Label>
              <Input
                id="share-table"
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
              className="px-0 text-xs text-primary/80"
              onClick={() => setShowAdvanced(value => !value)}
            >
              {showAdvanced ? t('export.hideAdvanced') : t('export.showAdvanced')}
            </Button>

            {showAdvanced && (
              <div className="mt-3 grid grid-cols-2 gap-3">
                {showBatchSize && (
                  <div className="space-y-2">
                    <Label htmlFor="share-batch">{t('export.batchLabel')}</Label>
                    <Input
                      id="share-batch"
                      type="number"
                      min={1}
                      value={batchSize}
                      onChange={event => setBatchSize(event.target.value)}
                    />
                  </div>
                )}
                <div className="space-y-2">
                  <Label htmlFor="share-limit">{t('export.limitLabel')}</Label>
                  <Input
                    id="share-limit"
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

          <div className="rounded-md border border-amber-500/30 bg-amber-500/10 p-3 text-sm text-amber-950 dark:text-amber-100">
            <p className="font-medium">{t('share.warningTitle')}</p>
            <p className="mt-1 text-xs opacity-90">{t('share.warningBody')}</p>
          </div>

          <Label className="flex items-start gap-2.5 text-sm cursor-pointer">
            <Checkbox
              checked={acknowledged}
              onCheckedChange={checked => setAcknowledged(Boolean(checked))}
              className="mt-0.5"
            />
            <span className="text-sm text-foreground">{t('share.confirmUpload')}</span>
          </Label>
        </div>

        <DialogFooter className="gap-2">
          <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button type="button" onClick={() => void handleSubmit()} disabled={submitting}>
            {submitting ? t('share.submitting') : t('share.generateLink')}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

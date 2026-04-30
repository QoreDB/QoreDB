// SPDX-License-Identifier: Apache-2.0

import { open as openDialog } from '@tauri-apps/plugin-dialog';
import { FileUp, Loader2 } from 'lucide-react';
import { useEffect, useState } from 'react';
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
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import { notify } from '@/lib/notify';
import {
  type CsvImportConfig,
  type ImportResponse,
  importCsv,
  type Namespace,
  previewCsv,
  type TableColumn,
} from '@/lib/tauri';

interface CSVImportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  tableColumns: TableColumn[];
  environment: string;
  readOnly: boolean;
  acknowledgedDangerous?: boolean;
  onSuccess?: () => void;
}

const IGNORE_COLUMN = '__ignore__';

export function CSVImportDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
  tableName,
  tableColumns,
  environment,
  readOnly,
  acknowledgedDangerous,
  onSuccess,
}: CSVImportDialogProps) {
  const { t } = useTranslation();

  // File & preview state
  const [filePath, setFilePath] = useState('');
  const [headers, setHeaders] = useState<string[]>([]);
  const [previewRows, setPreviewRows] = useState<string[][]>([]);
  const [totalLines, setTotalLines] = useState(0);
  const [loadingPreview, setLoadingPreview] = useState(false);

  // Config state
  const [delimiter, setDelimiter] = useState<string>('');
  const [hasHeader, setHasHeader] = useState(true);
  const [nullString, setNullString] = useState('');
  const [onConflict, setOnConflict] = useState<'skip' | 'abort'>('skip');
  const [columnMapping, setColumnMapping] = useState<Record<number, string>>({});

  // Import state
  const [importing, setImporting] = useState(false);

  // Reset state when dialog opens/closes
  useEffect(() => {
    if (!open) return;
    setFilePath('');
    setHeaders([]);
    setPreviewRows([]);
    setTotalLines(0);
    setDelimiter('');
    setHasHeader(true);
    setNullString('');
    setOnConflict('skip');
    setColumnMapping({});
    setImporting(false);
    setLoadingPreview(false);
  }, [open]);

  const handleSelectFile = async () => {
    const selected = await openDialog({
      multiple: false,
      filters: [
        {
          name: 'CSV',
          extensions: ['csv', 'tsv', 'txt'],
        },
      ],
    });

    if (selected) {
      const path = typeof selected === 'string' ? selected : selected;
      setFilePath(path);
      await loadPreview(path, delimiter || undefined, hasHeader);
    }
  };

  const loadPreview = async (path: string, delim?: string, header?: boolean) => {
    setLoadingPreview(true);
    try {
      const result = await previewCsv(path, delim, header, 5);
      setHeaders(result.headers);
      setPreviewRows(result.preview_rows);
      setTotalLines(result.total_lines);

      // Set detected delimiter if not manually set
      if (!delim) {
        setDelimiter(result.detected_delimiter);
      }

      // Auto-map columns by name match
      const mapping: Record<number, string> = {};
      for (let i = 0; i < result.headers.length; i++) {
        const csvHeader = result.headers[i].toLowerCase().trim();
        const match = tableColumns.find(col => col.name.toLowerCase() === csvHeader);
        if (match) {
          mapping[i] = match.name;
        } else {
          mapping[i] = IGNORE_COLUMN;
        }
      }
      setColumnMapping(mapping);
    } catch (err) {
      notify.error(t('import.previewError'), err);
    } finally {
      setLoadingPreview(false);
    }
  };

  const handleDelimiterChange = (value: string) => {
    setDelimiter(value);
    if (filePath) {
      loadPreview(filePath, value, hasHeader);
    }
  };

  const handleHeaderToggle = (checked: boolean) => {
    setHasHeader(checked);
    if (filePath) {
      loadPreview(filePath, delimiter || undefined, checked);
    }
  };

  const handleMappingChange = (csvIndex: number, tableCol: string) => {
    setColumnMapping(prev => ({ ...prev, [csvIndex]: tableCol }));
  };

  const handleImport = async () => {
    if (!filePath) {
      notify.error(t('import.noFile'));
      return;
    }

    // Build mapping (exclude ignored columns)
    const finalMapping: Record<number, string> = {};
    for (const [idx, col] of Object.entries(columnMapping)) {
      if (col !== IGNORE_COLUMN) {
        finalMapping[Number(idx)] = col;
      }
    }

    if (Object.keys(finalMapping).length === 0) {
      notify.error(t('import.noMapping'));
      return;
    }

    const config: CsvImportConfig = {
      delimiter: delimiter || undefined,
      has_header: hasHeader,
      null_string: nullString || undefined,
      on_conflict: onConflict,
      column_mapping: finalMapping,
    };

    setImporting(true);
    try {
      const result: ImportResponse = await importCsv(
        sessionId,
        namespace.database,
        namespace.schema,
        tableName,
        filePath,
        config,
        acknowledgedDangerous
      );

      if (result.success) {
        notify.success(t('import.success', { count: result.imported_rows }));
        onOpenChange(false);
        onSuccess?.();
      } else if (result.imported_rows > 0) {
        notify.warning(
          t('import.partial', {
            imported: result.imported_rows,
            failed: result.failed_rows,
          })
        );
        onSuccess?.();
      } else {
        notify.error(t('import.failed'), result.errors[0]);
      }
    } catch (err) {
      notify.error(t('import.failed'), err);
    } finally {
      setImporting(false);
    }
  };

  const hasPreview = headers.length > 0;
  const isProduction = environment === 'production';
  const fileName = filePath ? filePath.split('/').pop()?.split('\\').pop() : '';

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl max-h-[85vh] overflow-y-auto">
        <DialogHeader>
          <DialogTitle>
            {t('import.title')} — {tableName}
          </DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          {/* File selection */}
          <div className="space-y-2">
            <Label>{t('import.selectFile')}</Label>
            <div className="flex gap-2">
              <Input
                value={fileName || ''}
                readOnly
                placeholder={t('import.selectFile')}
                className="flex-1"
              />
              <Button
                variant="outline"
                type="button"
                onClick={handleSelectFile}
                disabled={importing}
              >
                <FileUp size={14} className="mr-2" />
                {t('import.browse')}
              </Button>
            </div>
          </div>

          {/* Delimiter & header options */}
          <div className="grid grid-cols-2 gap-3">
            <div className="space-y-2">
              <Label htmlFor="import-delimiter">{t('import.delimiter')}</Label>
              <Select value={delimiter} onValueChange={handleDelimiterChange}>
                <SelectTrigger id="import-delimiter">
                  <SelectValue placeholder={t('import.autoDetected')} />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value=",">,</SelectItem>
                  <SelectItem value=";">;</SelectItem>
                  <SelectItem value="\t">Tab</SelectItem>
                  <SelectItem value="|">|</SelectItem>
                </SelectContent>
              </Select>
            </div>

            <div className="flex items-end gap-2 pb-1">
              <Checkbox
                id="import-header"
                checked={hasHeader}
                onCheckedChange={checked => handleHeaderToggle(Boolean(checked))}
              />
              <Label htmlFor="import-header">{t('import.hasHeader')}</Label>
            </div>
          </div>

          {/* Loading indicator */}
          {loadingPreview && (
            <div className="flex items-center justify-center py-4 text-muted-foreground">
              <Loader2 size={16} className="mr-2 animate-spin" />
              {t('import.preview')}...
            </div>
          )}

          {/* Preview table */}
          {hasPreview && !loadingPreview && (
            <div className="space-y-2">
              <Label>
                {t('import.preview')} ({totalLines} {totalLines === 1 ? 'row' : 'rows'})
              </Label>
              <div className="overflow-x-auto rounded border border-border">
                <table className="w-full text-xs">
                  <thead>
                    <tr className="bg-muted/50">
                      {headers.map((h, i) => (
                        <th
                          key={i}
                          className="px-2 py-1 text-left font-medium text-muted-foreground"
                        >
                          {h}
                        </th>
                      ))}
                    </tr>
                  </thead>
                  <tbody>
                    {previewRows.map((row, ri) => (
                      <tr key={ri} className="border-t border-border">
                        {row.map((cell, ci) => (
                          <td key={ci} className="px-2 py-1 max-w-50 truncate">
                            {cell}
                          </td>
                        ))}
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
            </div>
          )}

          {/* Column mapping */}
          {hasPreview && !loadingPreview && (
            <div className="space-y-2">
              <Label>{t('import.columnMapping')}</Label>
              <div className="space-y-1">
                {headers.map((csvCol, idx) => (
                  <div key={idx} className="grid grid-cols-[1fr,auto,1fr] items-center gap-2">
                    <span className="text-sm truncate text-muted-foreground" title={csvCol}>
                      {csvCol}
                    </span>
                    <span className="text-xs text-muted-foreground">&rarr;</span>
                    <Select
                      value={columnMapping[idx] || IGNORE_COLUMN}
                      onValueChange={value => handleMappingChange(idx, value)}
                    >
                      <SelectTrigger className="h-8 text-xs">
                        <SelectValue />
                      </SelectTrigger>
                      <SelectContent>
                        <SelectItem value={IGNORE_COLUMN}>
                          <span className="text-muted-foreground italic">{t('import.ignore')}</span>
                        </SelectItem>
                        {tableColumns.map(col => (
                          <SelectItem key={col.name} value={col.name}>
                            {col.name}{' '}
                            <span className="text-muted-foreground">({col.data_type})</span>
                          </SelectItem>
                        ))}
                      </SelectContent>
                    </Select>
                  </div>
                ))}
              </div>
            </div>
          )}

          {/* Advanced options */}
          {hasPreview && !loadingPreview && (
            <div className="grid grid-cols-2 gap-3">
              <div className="space-y-2">
                <Label htmlFor="import-null">{t('import.nullString')}</Label>
                <Input
                  id="import-null"
                  value={nullString}
                  onChange={e => setNullString(e.target.value)}
                  placeholder="NULL"
                  className="h-8 text-xs"
                />
              </div>
              <div className="space-y-2">
                <Label htmlFor="import-conflict">{t('import.onConflict')}</Label>
                <Select
                  value={onConflict}
                  onValueChange={v => setOnConflict(v as 'skip' | 'abort')}
                >
                  <SelectTrigger id="import-conflict" className="h-8 text-xs">
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="skip">{t('import.skip')}</SelectItem>
                    <SelectItem value="abort">{t('import.abort')}</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            </div>
          )}

          {/* Production warning */}
          {isProduction && hasPreview && (
            <p className="text-xs text-warning">{t('import.productionWarning')}</p>
          )}
        </div>

        <DialogFooter className="gap-2">
          <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button
            type="button"
            onClick={handleImport}
            disabled={importing || !hasPreview || readOnly}
          >
            {importing ? (
              <>
                <Loader2 size={14} className="mr-2 animate-spin" />
                {t('import.importing')}
              </>
            ) : (
              t('import.start')
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

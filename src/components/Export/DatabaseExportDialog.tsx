// SPDX-License-Identifier: Apache-2.0

import { save } from '@tauri-apps/plugin-dialog';
import { Loader2 } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
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
  cancelDatabaseExport,
  type DatabaseExportFormat,
  type DatabaseExportProgress,
  databaseExportProgressEvent,
  exportDatabaseFull,
  type Namespace,
} from '@/lib/tauri';
import { listen, type UnlistenFn } from '@/lib/transport';

interface DatabaseExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
}

export function DatabaseExportDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
}: DatabaseExportDialogProps) {
  const { t } = useTranslation();

  const [outputPath, setOutputPath] = useState('');
  const [format, setFormat] = useState<DatabaseExportFormat>('sql');
  const [includeSchema, setIncludeSchema] = useState(true);
  const [includeData, setIncludeData] = useState(true);
  const [progress, setProgress] = useState<DatabaseExportProgress | null>(null);

  const exporting = progress?.state === 'pending' || progress?.state === 'running';
  const unlistenRef = useRef<UnlistenFn | null>(null);
  const exportIdRef = useRef<string | null>(null);

  useEffect(() => {
    if (!open) return;
    setOutputPath('');
    setFormat('sql');
    setIncludeSchema(true);
    setIncludeData(true);
    setProgress(null);
  }, [open]);

  useEffect(
    () => () => {
      unlistenRef.current?.();
      unlistenRef.current = null;
    },
    []
  );

  const handleBrowse = async () => {
    const ext = format === 'zip' ? 'zip' : 'sql';
    const defaultName = `${namespace.database}${namespace.schema ? `_${namespace.schema}` : ''}.${ext}`;
    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (filePath) {
      setOutputPath(filePath);
    }
  };

  const cleanup = () => {
    unlistenRef.current?.();
    unlistenRef.current = null;
    exportIdRef.current = null;
  };

  const handleExport = async () => {
    if (!outputPath) {
      notify.error(t('databaseExport.noPath'));
      return;
    }
    if (!includeSchema && !includeData) {
      notify.error(t('databaseExport.nothingSelected'));
      return;
    }

    const exportId =
      crypto.randomUUID?.() ?? `${Date.now()}-${Math.random().toString(16).slice(2)}`;
    exportIdRef.current = exportId;

    try {
      const unlisten = await listen<DatabaseExportProgress>(
        databaseExportProgressEvent(exportId),
        event => {
          const payload = event.payload;
          setProgress(payload);
          if (
            payload.state === 'completed' ||
            payload.state === 'failed' ||
            payload.state === 'cancelled'
          ) {
            cleanup();
            if (payload.state === 'completed') {
              notify.success(
                t('databaseExport.success', {
                  tables: payload.tables_total,
                  rows: payload.rows_exported,
                })
              );
              onOpenChange(false);
            } else if (payload.state === 'cancelled') {
              notify.info(t('databaseExport.cancelled'));
            } else {
              notify.error(t('databaseExport.failed'), payload.error ?? undefined);
            }
          }
        }
      );
      unlistenRef.current = unlisten;

      setProgress({
        export_id: exportId,
        state: 'pending',
        tables_done: 0,
        tables_total: 0,
        rows_exported: 0,
        bytes_written: 0,
        elapsed_ms: 0,
      });

      await exportDatabaseFull(
        sessionId,
        namespace.database,
        namespace.schema,
        outputPath,
        format,
        {
          include_schema: includeSchema,
          include_data: includeData,
        },
        exportId
      );
    } catch (err) {
      cleanup();
      setProgress(null);
      notify.error(t('databaseExport.failed'), err);
    }
  };

  const handleCancel = () => {
    const id = exportIdRef.current;
    if (id) {
      cancelDatabaseExport(id).catch(() => undefined);
    }
  };

  const progressLabel = () => {
    if (!progress) return '';
    if (progress.tables_total > 0) {
      return t('databaseExport.progress', {
        current: progress.current_table ?? '',
        done: progress.tables_done,
        total: progress.tables_total,
        rows: progress.rows_exported,
      });
    }
    return t('databaseExport.preparing');
  };

  return (
    <Dialog open={open} onOpenChange={o => !exporting && onOpenChange(o)}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('databaseExport.title')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <p className="text-sm text-muted-foreground">{t('databaseExport.description')}</p>

          {/* Output file */}
          <div className="space-y-2">
            <Label>{t('databaseExport.outputFile')}</Label>
            <div className="flex gap-2">
              <Input
                value={outputPath}
                onChange={e => setOutputPath(e.target.value)}
                placeholder={`${namespace.database}.${format === 'zip' ? 'zip' : 'sql'}`}
                className="flex-1"
                disabled={exporting}
              />
              <Button variant="outline" type="button" onClick={handleBrowse} disabled={exporting}>
                {t('import.browse')}
              </Button>
            </div>
          </div>

          {/* Format */}
          <div className="space-y-2">
            <Label>{t('databaseExport.format')}</Label>
            <Select
              value={format}
              onValueChange={v => setFormat(v as DatabaseExportFormat)}
              disabled={exporting}
            >
              <SelectTrigger>
                <SelectValue />
              </SelectTrigger>
              <SelectContent>
                <SelectItem value="sql">{t('databaseExport.formatSql')}</SelectItem>
                <SelectItem value="zip">{t('databaseExport.formatZip')}</SelectItem>
              </SelectContent>
            </Select>
            <p className="text-xs text-muted-foreground">
              {format === 'zip'
                ? t('databaseExport.formatZipHint')
                : t('databaseExport.formatSqlHint')}
            </p>
          </div>

          {/* Include options */}
          <div className="space-y-2">
            <Label>{t('databaseExport.include')}</Label>
            <div className="flex items-center gap-2">
              <Checkbox
                id="db-export-schema"
                checked={includeSchema}
                onCheckedChange={c => setIncludeSchema(Boolean(c))}
                disabled={exporting}
              />
              <Label className="mb-0" htmlFor="db-export-schema">
                {t('databaseExport.includeSchema')}
              </Label>
            </div>
            <div className="flex items-center gap-2">
              <Checkbox
                id="db-export-data"
                checked={includeData}
                onCheckedChange={c => setIncludeData(Boolean(c))}
                disabled={exporting}
              />
              <Label className="mb-0" htmlFor="db-export-data">
                {t('databaseExport.includeData')}
              </Label>
            </div>
          </div>

          {exporting && (
            <div className="rounded-md border border-border bg-muted/30 px-3 py-2 text-sm text-muted-foreground">
              <span className="flex items-center gap-2">
                <Loader2 size={14} className="animate-spin" />
                {progressLabel()}
              </span>
            </div>
          )}
        </div>

        <DialogFooter className="gap-2">
          {exporting ? (
            <Button variant="ghost" type="button" onClick={handleCancel}>
              {t('common.cancel')}
            </Button>
          ) : (
            <>
              <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
                {t('common.cancel')}
              </Button>
              <Button type="button" onClick={handleExport} disabled={!outputPath}>
                {t('databaseExport.export')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

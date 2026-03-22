// SPDX-License-Identifier: Apache-2.0

import { save } from '@tauri-apps/plugin-dialog';
import { Loader2 } from 'lucide-react';
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
import { notify } from '@/lib/notify';
import { exportSchema, type Namespace, type SchemaExportOptions } from '@/lib/tauri';

interface SchemaExportDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
  supportsRoutines: boolean;
  supportsTriggers: boolean;
  supportsEvents: boolean;
}

export function SchemaExportDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
  supportsRoutines,
  supportsTriggers,
  supportsEvents,
}: SchemaExportDialogProps) {
  const { t } = useTranslation();

  const [outputPath, setOutputPath] = useState('');
  const [includeTables, setIncludeTables] = useState(true);
  const [includeRoutines, setIncludeRoutines] = useState(true);
  const [includeTriggers, setIncludeTriggers] = useState(true);
  const [includeEvents, setIncludeEvents] = useState(true);
  const [exporting, setExporting] = useState(false);

  useEffect(() => {
    if (!open) return;
    setOutputPath('');
    setIncludeTables(true);
    setIncludeRoutines(true);
    setIncludeTriggers(true);
    setIncludeEvents(true);
    setExporting(false);
  }, [open]);

  const handleBrowse = async () => {
    const defaultName = `${namespace.database}${namespace.schema ? `_${namespace.schema}` : ''}_schema.sql`;
    const filePath = await save({
      defaultPath: defaultName,
      filters: [{ name: 'SQL', extensions: ['sql'] }],
    });
    if (filePath) {
      setOutputPath(filePath);
    }
  };

  const handleExport = async () => {
    if (!outputPath) {
      notify.error(t('schemaExport.noPath'));
      return;
    }

    const options: SchemaExportOptions = {
      include_tables: includeTables,
      include_routines: includeRoutines,
      include_triggers: includeTriggers,
      include_events: includeEvents,
    };

    setExporting(true);
    try {
      const result = await exportSchema(
        sessionId,
        namespace.database,
        namespace.schema,
        outputPath,
        options
      );

      if (result.success) {
        const parts: string[] = [];
        if (result.table_count > 0) parts.push(`${result.table_count} tables`);
        if (result.routine_count > 0) parts.push(`${result.routine_count} routines`);
        if (result.trigger_count > 0) parts.push(`${result.trigger_count} triggers`);
        if (result.event_count > 0) parts.push(`${result.event_count} events`);

        notify.success(
          t('schemaExport.success', {
            tables: result.table_count,
            routines: result.routine_count,
          })
        );
        onOpenChange(false);
      } else {
        notify.error(t('schemaExport.failed'), result.error);
      }
    } catch (err) {
      notify.error(t('schemaExport.failed'), err);
    } finally {
      setExporting(false);
    }
  };

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-md">
        <DialogHeader>
          <DialogTitle>{t('schemaExport.title')}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4">
          <p className="text-sm text-muted-foreground">{t('schemaExport.description')}</p>

          {/* Output file */}
          <div className="space-y-2">
            <Label>{t('schemaExport.outputFile')}</Label>
            <div className="flex gap-2">
              <Input
                value={outputPath}
                onChange={e => setOutputPath(e.target.value)}
                placeholder={`${namespace.database}_schema.sql`}
                className="flex-1"
              />
              <Button variant="outline" type="button" onClick={handleBrowse}>
                {t('import.browse')}
              </Button>
            </div>
          </div>

          {/* Include options */}
          <div className="space-y-2">
            <Label>{t('schemaExport.include')}</Label>
            <div className="space-y-2">
              <div className="flex items-center gap-2">
                <Checkbox
                  id="schema-tables"
                  checked={includeTables}
                  onCheckedChange={c => setIncludeTables(Boolean(c))}
                />
                <Label htmlFor="schema-tables">{t('schemaExport.includeTables')}</Label>
              </div>

              {supportsRoutines && (
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="schema-routines"
                    className="mb-0"
                    checked={includeRoutines}
                    onCheckedChange={c => setIncludeRoutines(Boolean(c))}
                  />
                  <Label htmlFor="schema-routines">{t('schemaExport.includeRoutines')}</Label>
                </div>
              )}

              {supportsTriggers && (
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="schema-triggers"
                    checked={includeTriggers}
                    onCheckedChange={c => setIncludeTriggers(Boolean(c))}
                  />
                  <Label className="mb-0" htmlFor="schema-triggers">
                    {t('schemaExport.includeTriggers')}
                  </Label>
                </div>
              )}

              {supportsEvents && (
                <div className="flex items-center gap-2">
                  <Checkbox
                    id="schema-events"
                    checked={includeEvents}
                    onCheckedChange={c => setIncludeEvents(Boolean(c))}
                  />
                  <Label className="mb-0" htmlFor="schema-events">
                    {t('schemaExport.includeEvents')}
                  </Label>
                </div>
              )}
            </div>
          </div>
        </div>

        <DialogFooter className="gap-2">
          <Button variant="ghost" type="button" onClick={() => onOpenChange(false)}>
            {t('common.cancel')}
          </Button>
          <Button type="button" onClick={handleExport} disabled={exporting || !outputPath}>
            {exporting ? (
              <>
                <Loader2 size={14} className="mr-2 animate-spin" />
                {t('schemaExport.exporting')}
              </>
            ) : (
              t('schemaExport.export')
            )}
          </Button>
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

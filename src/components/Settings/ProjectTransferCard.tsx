import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { save, open as openDialog } from '@tauri-apps/plugin-dialog';
import { readTextFile, writeTextFile } from '@tauri-apps/plugin-fs';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { toast } from 'sonner';
import { Briefcase, Upload } from 'lucide-react';

import { Checkbox } from '@/components/ui/checkbox';
import { Button } from '@/components/ui/button';
import { SettingsCard } from './SettingsCard';
import { buildProjectExportV1, importProjectExportV1, isProjectExportV1 } from '@/lib/projectTransfer';

interface ProjectTransferCardProps {
  projectId: string;
}

const MAX_PROJECT_BYTES = 5_000_000;

export function ProjectTransferCard({ projectId }: ProjectTransferCardProps) {
  const { t } = useTranslation();
  const [exporting, setExporting] = useState(false);
  const [importing, setImporting] = useState(false);
  const [includeLibrary, setIncludeLibrary] = useState(true);
  const [redactQueries, setRedactQueries] = useState(true);

  async function handleExport() {
    setExporting(true);
    try {
      const payload = await buildProjectExportV1({
        projectId,
        includeQueryLibrary: includeLibrary,
        redactQueries,
      });

      const filePath = await save({
        defaultPath: 'qoredb-project.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath) return;

      await writeTextFile(filePath, JSON.stringify(payload, null, 2));
      revealItemInDir(filePath).catch(() => undefined);

      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('settings.projectExportSuccess', { name }), {
        description: filePath,
      });
    } catch (err) {
      toast.error(t('settings.projectExportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setExporting(false);
    }
  }

  async function handleImport() {
    if (!confirm(t('settings.projectImportConfirm'))) return;

    setImporting(true);
    try {
      const filePath = await openDialog({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || Array.isArray(filePath)) return;

      const raw = await readTextFile(filePath);
      if (raw.length > MAX_PROJECT_BYTES) {
        throw new Error(t('settings.projectTooLarge'));
      }

      const parsed: unknown = JSON.parse(raw);
      if (!isProjectExportV1(parsed)) {
        throw new Error(t('settings.projectInvalid'));
      }

      const result = await importProjectExportV1(parsed, { projectId });
      if (result.connectionsImported > 0) {
        window.dispatchEvent(new Event('qoredb:connections-changed'));
      }

      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(
        t('settings.projectImportSuccess', {
          name,
          connections: result.connectionsImported,
        }),
        {
          description:
            result.connectionsSkipped > 0
              ? t('settings.projectImportPartial', { skipped: result.connectionsSkipped })
              : undefined,
        }
      );
    } catch (err) {
      toast.error(t('settings.projectImportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      setImporting(false);
    }
  }

  return (
    <SettingsCard
      title={t('settings.projectTransfer')}
      description={t('settings.projectTransferDescription')}
    >
      <div className="space-y-4">
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={handleExport} disabled={exporting}>
            <Briefcase size={16} className="mr-2" />
            {t('settings.projectExport')}
          </Button>
          <Button variant="outline" onClick={handleImport} disabled={importing}>
            <Upload size={16} className="mr-2" />
            {t('settings.projectImport')}
          </Button>
        </div>

        <label className="flex items-start gap-3 text-sm">
          <Checkbox checked={includeLibrary} onCheckedChange={checked => setIncludeLibrary(!!checked)} />
          <span>
            <span className="font-medium">{t('settings.projectIncludeLibrary')}</span>
            <span className="block text-xs text-muted-foreground">
              {t('settings.projectIncludeLibraryDescription')}
            </span>
          </span>
        </label>

        <label className="flex items-start gap-3 text-sm">
          <Checkbox
            checked={redactQueries}
            disabled={!includeLibrary}
            onCheckedChange={checked => setRedactQueries(!!checked)}
          />
          <span>
            <span className="font-medium">{t('settings.projectRedactQueries')}</span>
            <span className="block text-xs text-muted-foreground">
              {t('settings.projectRedactQueriesDescription')}
            </span>
          </span>
        </label>
      </div>
    </SettingsCard>
  );
}


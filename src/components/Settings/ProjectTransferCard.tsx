// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save } from '@tauri-apps/plugin-dialog';
import { readTextFile } from '@tauri-apps/plugin-fs';
import { revealItemInDir } from '@tauri-apps/plugin-opener';
import { Briefcase, Upload } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import { Checkbox } from '@/components/ui/checkbox';
import { emitUiEvent, UI_EVENT_CONNECTIONS_CHANGED } from '@/lib/events/uiEvents';
import {
  buildProjectExportV1,
  importProjectExportV1,
  isProjectExportV1,
} from '@/lib/share/projectTransfer';
import { confirmDialog } from '@/lib/stores/confirmStore';
import { captureWorkspaceScope } from '@/lib/stores/workspaceStore';
import { writeTextFileAtomic } from '@/lib/tauri/fileOutput';
import { SettingsCard } from './SettingsCard';

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

  const busy = useRef(false);
  const mounted = useRef(true);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);

  async function handleExport() {
    const inWorkspace = captureWorkspaceScope(projectId);
    const isCurrent = () => mounted.current && inWorkspace();
    if (busy.current || !isCurrent()) return;
    busy.current = true;
    setExporting(true);
    try {
      const payload = await buildProjectExportV1({
        projectId,
        includeQueryLibrary: includeLibrary,
        redactQueries,
      });

      if (!isCurrent()) return;
      const filePath = await save({
        defaultPath: 'qoredb-project.json',
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || !isCurrent()) return;

      await writeTextFileAtomic(filePath, JSON.stringify(payload, null, 2));
      if (!isCurrent()) return;
      revealItemInDir(filePath).catch(() => undefined);

      const name = filePath.split(/[\\/]/).pop() || filePath;
      toast.success(t('settings.projectExportSuccess', { name }), {
        description: filePath,
      });
    } catch (err) {
      if (!isCurrent()) return;
      toast.error(t('settings.projectExportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      busy.current = false;
      if (mounted.current) setExporting(false);
    }
  }

  async function handleImport() {
    const inWorkspace = captureWorkspaceScope(projectId);
    const isCurrent = () => mounted.current && inWorkspace();
    if (busy.current || !isCurrent()) return;
    busy.current = true;
    setImporting(true);
    try {
      if (
        !(await confirmDialog({ description: t('settings.projectImportConfirm') })) ||
        !isCurrent()
      )
        return;
      const filePath = await openDialog({
        multiple: false,
        filters: [{ name: 'JSON', extensions: ['json'] }],
      });
      if (!filePath || Array.isArray(filePath) || !isCurrent()) return;

      const raw = await readTextFile(filePath);
      if (!isCurrent()) return;
      if (raw.length > MAX_PROJECT_BYTES) {
        throw new Error(t('settings.projectTooLarge'));
      }

      const parsed: unknown = JSON.parse(raw);
      if (!isProjectExportV1(parsed)) {
        throw new Error(t('settings.projectInvalid'));
      }

      const result = await importProjectExportV1(parsed, { projectId });
      if (!isCurrent()) return;
      if (result.connectionsImported > 0) {
        emitUiEvent(UI_EVENT_CONNECTIONS_CHANGED);
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
      if (!isCurrent()) return;
      toast.error(t('settings.projectImportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    } finally {
      busy.current = false;
      if (mounted.current) setImporting(false);
    }
  }

  return (
    <SettingsCard
      title={t('settings.projectTransfer')}
      description={t('settings.projectTransferDescription')}
    >
      <div className="space-y-4">
        <div className="flex flex-wrap gap-2">
          <Button variant="outline" onClick={handleExport} disabled={exporting || importing}>
            <Briefcase size={16} className="mr-2" />
            {t('settings.projectExport')}
          </Button>
          <Button variant="outline" onClick={handleImport} disabled={exporting || importing}>
            <Upload size={16} className="mr-2" />
            {t('settings.projectImport')}
          </Button>
        </div>

        <label className="flex items-start gap-3 text-sm">
          <Checkbox
            checked={includeLibrary}
            onCheckedChange={checked => setIncludeLibrary(!!checked)}
          />
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

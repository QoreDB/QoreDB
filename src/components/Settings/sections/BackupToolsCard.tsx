// SPDX-License-Identifier: Apache-2.0

import { open } from '@tauri-apps/plugin-dialog';
import { CheckCircle2, FolderOpen, RefreshCw, XCircle } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  type BackupTool,
  type BackupToolInfo,
  detectBackupTools,
  setBackupToolPath,
} from '@/lib/tauri/backup';
import { SettingsCard } from '../SettingsCard';

interface BackupToolsCardProps {
  searchQuery?: string;
}

const TOOL_ORDER: BackupTool[] = [
  'pg_dump',
  'pg_restore',
  'psql',
  'mysql_dump',
  'maria_db_dump',
  'mysql',
  'mongo_dump',
  'mongo_restore',
  'sqlite3',
];

export function BackupToolsCard({ searchQuery }: BackupToolsCardProps) {
  const { t } = useTranslation();
  const [tools, setTools] = useState<BackupToolInfo[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const next = await detectBackupTools();
      const order = new Map(TOOL_ORDER.map((tool, idx) => [tool, idx]));
      next.sort((a, b) => (order.get(a.tool) ?? 99) - (order.get(b.tool) ?? 99));
      setTools(next);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  const overrideTool = useCallback(
    async (tool: BackupTool) => {
      const chosen = await open({ multiple: false });
      if (typeof chosen !== 'string') return;
      try {
        await setBackupToolPath(tool, chosen);
        await refresh();
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      }
    },
    [refresh]
  );

  const isModified = tools.some(info => info.overridden);

  return (
    <SettingsCard
      id="backup-tools"
      title={t('settings.backupTools.title')}
      description={t('settings.backupTools.description')}
      isModified={isModified}
      searchQuery={searchQuery}
    >
      <div className="space-y-3">
        <div className="flex items-center justify-between">
          <p className="text-xs text-muted-foreground">
            {t('settings.backupTools.summary', {
              found: tools.filter(t => !!t.path).length,
              total: tools.length,
            })}
          </p>
          <Button variant="outline" size="sm" onClick={refresh} disabled={loading}>
            <RefreshCw size={12} className={loading ? 'animate-spin' : ''} />
            {t('settings.backupTools.refresh')}
          </Button>
        </div>

        {error && <p className="text-xs text-destructive">{error}</p>}

        <div className="rounded-md border border-border divide-y divide-border">
          {tools.map(info => (
            <ToolRow key={info.tool} info={info} onOverride={overrideTool} />
          ))}
          {tools.length === 0 && !loading && (
            <p className="text-xs text-muted-foreground p-3">{t('settings.backupTools.empty')}</p>
          )}
        </div>
      </div>
    </SettingsCard>
  );
}

interface ToolRowProps {
  info: BackupToolInfo;
  onOverride: (tool: BackupTool) => void;
}

function ToolRow({ info, onOverride }: ToolRowProps) {
  const { t } = useTranslation();
  const found = !!info.path;

  return (
    <div className="flex items-center justify-between gap-3 px-3 py-2">
      <div className="min-w-0 flex-1">
        <div className="flex items-center gap-2">
          {found ? (
            <CheckCircle2 size={12} className="text-green-500 shrink-0" />
          ) : (
            <XCircle size={12} className="text-muted-foreground shrink-0" />
          )}
          <span className="text-sm font-medium text-foreground">{info.binary_name}</span>
          {info.overridden && (
            <span className="px-1.5 py-0.5 text-[10px] font-medium uppercase tracking-wider bg-primary/15 text-primary rounded">
              {t('settings.backupTools.overridden')}
            </span>
          )}
        </div>
        <p className="text-xs text-muted-foreground mt-0.5 truncate font-mono">
          {info.path ?? t('settings.backupTools.notFound')}
        </p>
      </div>
      <Button variant="outline" size="sm" onClick={() => onOverride(info.tool)}>
        <FolderOpen size={12} />
        {t('settings.backupTools.choose')}
      </Button>
    </div>
  );
}

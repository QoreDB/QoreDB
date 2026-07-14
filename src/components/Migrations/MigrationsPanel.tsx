// SPDX-License-Identifier: Apache-2.0

import {
  AlertTriangle,
  FileCode,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Trash2,
  Undo2,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { loadMigrations, useMigrationsStore } from '@/lib/migrations/migrationsStore';
import {
  buildMigrationFilename,
  nextVersion,
  parseMigration,
  serializeMigration,
  slugify,
} from '@/lib/migrations/parse';
import { notify } from '@/lib/notify';
import { confirmDialog } from '@/lib/stores/confirmStore';
import {
  applyMigration,
  getMigrationStatus,
  type MigrationDirection,
  type MigrationStatusEntry,
  wsDeleteMigration,
  wsReadMigration,
  wsWriteMigration,
} from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { useWorkspace } from '@/providers/WorkspaceProvider';

interface MigrationsPanelProps {
  sessionId?: string;
  database?: string;
  driver?: string;
  environment?: string;
  readOnly?: boolean;
}

// Drivers whose raw-SQL DDL the runner can apply. Excludes document/KV/search
// stores and ClickHouse (non-standard transactions/DDL).
const MIGRATION_DRIVERS = new Set([
  'postgres',
  'cockroachdb',
  'mysql',
  'mariadb',
  'sqlite',
  'duckdb',
  'sqlserver',
  'timescaledb',
  'supabase',
  'neon',
]);
// DDL auto-commits here, so a failed migration can't be fully rolled back.
const NON_TX_DDL_DRIVERS = new Set(['mysql', 'mariadb']);

export function MigrationsPanel({
  sessionId,
  database,
  driver,
  environment,
  readOnly,
}: MigrationsPanelProps) {
  const { t } = useTranslation();
  const { activeWorkspace, projectId } = useWorkspace();
  const migrations = useMigrationsStore(s => s.migrations);
  const isLoading = useMigrationsStore(s => s.isLoading);

  const [newName, setNewName] = useState('');
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ up: string; down: string } | null>(null);
  const [statusByVersion, setStatusByVersion] = useState<Record<string, MigrationStatusEntry>>({});
  const [applying, setApplying] = useState<string | null>(null);

  const isDefault = activeWorkspace == null || activeWorkspace.source === 'default';
  const driverSupported = driver != null && MIGRATION_DRIVERS.has(driver);
  const canApply = !!sessionId && driverSupported && !readOnly;

  // biome-ignore lint/correctness/useExhaustiveDependencies: `projectId` is intentional — reload migrations when switching between file-based workspaces (isDefault stays false)
  useEffect(() => {
    if (!isDefault) void loadMigrations();
  }, [isDefault, projectId]);

  const refreshStatus = useCallback(async () => {
    if (!sessionId) {
      setStatusByVersion({});
      return;
    }
    try {
      const entries = await getMigrationStatus(sessionId);
      setStatusByVersion(entries ? Object.fromEntries(entries.map(e => [e.version, e])) : {});
    } catch {
      setStatusByVersion({});
    }
  }, [sessionId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `migrations` is intentional — refresh applied status (and checksum drift) when the file list reloads
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus, migrations]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `migrations` is intentional — re-read the open file when the list reloads after an external edit
  useEffect(() => {
    if (!selected) {
      setDetail(null);
      return;
    }
    let cancelled = false;
    wsReadMigration(selected)
      .then(content => {
        if (!cancelled) setDetail(content == null ? null : parseMigration(content));
      })
      .catch(() => {
        if (!cancelled) setDetail(null);
      });
    return () => {
      cancelled = true;
    };
  }, [selected, migrations]);

  const handleCreate = useCallback(async () => {
    const name = newName.trim();
    if (!name) return;
    setCreating(true);
    try {
      const filename = buildMigrationFilename(nextVersion(migrations ?? []), slugify(name));
      const ok = await wsWriteMigration(filename, serializeMigration('', ''));
      if (!ok) {
        notify.error(t('migrations.requiresWorkspace'));
        return;
      }
      setNewName('');
      await loadMigrations();
      setSelected(filename);
      notify.success(t('migrations.created'));
    } catch (err) {
      notify.error(t('common.unknownError'));
      console.error('Failed to create migration:', err);
    } finally {
      setCreating(false);
    }
  }, [migrations, newName, t]);

  const handleDelete = useCallback(
    async (filename: string) => {
      const confirmed = await confirmDialog({
        title: t('migrations.delete'),
        description: t('migrations.deleteConfirm', { name: filename }),
        confirmLabel: t('migrations.delete'),
      });
      if (!confirmed) return;
      try {
        await wsDeleteMigration(filename);
        if (selected === filename) setSelected(null);
        await loadMigrations();
        notify.success(t('migrations.deleted'));
      } catch (err) {
        notify.error(t('common.unknownError'));
        console.error('Failed to delete migration:', err);
      }
    },
    [selected, t]
  );

  const handleApply = useCallback(
    async (filename: string, direction: MigrationDirection) => {
      if (!sessionId) return;
      const isUp = direction === 'up';
      const confirmed = await confirmDialog({
        title: isUp ? t('migrations.applyTitle') : t('migrations.rollbackTitle'),
        description: t(isUp ? 'migrations.applyConfirm' : 'migrations.rollbackConfirm', {
          name: filename,
        }),
        warningInfo: environment === 'production' ? t('migrations.prodWarning') : undefined,
        confirmLabel: isUp ? t('migrations.apply') : t('migrations.rollback'),
      });
      if (!confirmed) return;
      setApplying(filename);
      try {
        const res = await applyMigration(sessionId, filename, direction, database ?? '', true);
        if (res.success) {
          notify.success(isUp ? t('migrations.applied') : t('migrations.rolledBack'));
          await refreshStatus();
        } else {
          notify.error(res.error ?? t('common.unknownError'));
        }
      } catch (err) {
        notify.error(t('common.unknownError'));
        console.error('Failed to apply migration:', err);
      } finally {
        setApplying(null);
      }
    },
    [sessionId, database, environment, refreshStatus, t]
  );

  if (isDefault) {
    return (
      <div className="flex-1 min-h-0 flex flex-col items-center justify-center gap-2 p-8 text-center">
        <FileCode className="w-10 h-10 text-muted-foreground/50" />
        <h2 className="text-lg font-medium">{t('migrations.requiresWorkspace')}</h2>
        <p className="max-w-md text-sm text-muted-foreground">
          {t('migrations.requiresWorkspaceHint')}
        </p>
      </div>
    );
  }

  const list = migrations ?? [];

  return (
    <div className="flex-1 min-h-0 flex flex-col">
      <div className="flex items-center gap-2 px-4 py-3 border-b border-border">
        <div className="flex-1 min-w-0">
          <h2 className="text-sm font-semibold">{t('migrations.title')}</h2>
          <p className="text-xs text-muted-foreground truncate">
            {!sessionId
              ? t('migrations.connectHint')
              : !driverSupported
                ? t('migrations.driverUnsupported')
                : t('migrations.description')}
          </p>
        </div>
        <Input
          value={newName}
          onChange={e => setNewName(e.target.value)}
          onKeyDown={e => {
            if (e.key === 'Enter') void handleCreate();
          }}
          placeholder={t('migrations.newPlaceholder')}
          className="w-56"
        />
        <Button
          onClick={() => void handleCreate()}
          disabled={creating || !newName.trim()}
          size="sm"
        >
          {creating ? <Loader2 className="w-4 h-4 animate-spin" /> : <Plus className="w-4 h-4" />}
          {t('migrations.create')}
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={() => {
            void loadMigrations();
            void refreshStatus();
          }}
          title={t('migrations.refresh')}
        >
          <RefreshCw className={cn('w-4 h-4', isLoading && 'animate-spin')} />
        </Button>
      </div>

      {canApply && NON_TX_DDL_DRIVERS.has(driver ?? '') && (
        <div className="flex items-center gap-2 px-4 py-2 text-xs bg-amber-500/10 text-amber-700 dark:text-amber-400 border-b border-border">
          <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
          {t('migrations.mysqlCaveat')}
        </div>
      )}

      <div className="flex-1 min-h-0 flex">
        <div className="w-80 shrink-0 border-r border-border overflow-y-auto">
          {list.length === 0 ? (
            <div className="p-6 text-center text-sm text-muted-foreground">
              {t('migrations.empty')}
            </div>
          ) : (
            list.map(m => {
              const status = statusByVersion[m.version];
              const applied = status?.status === 'applied';
              const busy = applying === m.filename;
              return (
                <div
                  key={m.filename}
                  className={cn(
                    'flex items-center border-b border-border/50',
                    selected === m.filename && 'bg-muted'
                  )}
                >
                  <button
                    type="button"
                    onClick={() => setSelected(m.filename)}
                    className="flex-1 min-w-0 text-left px-3 py-2 flex items-center gap-2 hover:bg-muted/50"
                  >
                    <span className="font-mono text-xs text-muted-foreground">{m.version}</span>
                    <span className="flex-1 min-w-0 truncate text-sm">{m.name}</span>
                    {status && <StatusBadge status={status.status} />}
                    {status?.checksum_mismatch && (
                      <span title={t('migrations.checksumMismatch')} className="shrink-0">
                        <AlertTriangle className="w-3.5 h-3.5 text-amber-500" />
                      </span>
                    )}
                  </button>
                  {canApply && (
                    <button
                      type="button"
                      onClick={() => void handleApply(m.filename, applied ? 'down' : 'up')}
                      disabled={applying !== null}
                      title={applied ? t('migrations.rollback') : t('migrations.apply')}
                      className="px-2 py-2 text-muted-foreground hover:text-foreground disabled:opacity-50"
                    >
                      {busy ? (
                        <Loader2 className="w-3.5 h-3.5 animate-spin" />
                      ) : applied ? (
                        <Undo2 className="w-3.5 h-3.5" />
                      ) : (
                        <Play className="w-3.5 h-3.5" />
                      )}
                    </button>
                  )}
                  <button
                    type="button"
                    onClick={() => void handleDelete(m.filename)}
                    title={t('migrations.delete')}
                    className="px-2 py-2 text-muted-foreground hover:text-destructive"
                  >
                    <Trash2 className="w-3.5 h-3.5" />
                  </button>
                </div>
              );
            })
          )}
        </div>

        <div className="flex-1 min-w-0 overflow-y-auto p-4">
          {detail == null ? (
            <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
              {t('migrations.selectHint')}
            </div>
          ) : (
            <div className="flex flex-col gap-4">
              <section>
                <div className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                  {t('migrations.upLabel')}
                </div>
                <pre className="text-xs font-mono bg-muted/40 rounded p-3 whitespace-pre-wrap break-words min-h-16">
                  {detail.up || '—'}
                </pre>
              </section>
              <section>
                <div className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                  {t('migrations.downLabel')}
                </div>
                <pre className="text-xs font-mono bg-muted/40 rounded p-3 whitespace-pre-wrap break-words min-h-16">
                  {detail.down || '—'}
                </pre>
              </section>
            </div>
          )}
        </div>
      </div>
    </div>
  );
}

function StatusBadge({ status }: { status: MigrationStatusEntry['status'] }) {
  const { t } = useTranslation();
  const styles: Record<MigrationStatusEntry['status'], string> = {
    applied: 'bg-emerald-500/15 text-emerald-700 dark:text-emerald-400',
    pending: 'bg-muted text-muted-foreground',
    rolled_back: 'bg-amber-500/15 text-amber-700 dark:text-amber-400',
  };
  const labels: Record<MigrationStatusEntry['status'], string> = {
    applied: t('migrations.statusApplied'),
    pending: t('migrations.statusPending'),
    rolled_back: t('migrations.statusRolledBack'),
  };
  return (
    <span className={cn('shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium', styles[status])}>
      {labels[status]}
    </span>
  );
}

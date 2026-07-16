// SPDX-License-Identifier: Apache-2.0

import type { TFunction } from 'i18next';
import {
  AlertTriangle,
  Camera,
  Database,
  FileCode,
  Loader2,
  Play,
  Plus,
  RefreshCw,
  Save,
  ScanSearch,
  Trash2,
  Undo2,
  Wand2,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { SQLEditor } from '@/components/Editor/SQLEditor';
import { UpgradePrompt } from '@/components/License/UpgradePrompt';
import { translateDdlWarning } from '@/components/Table/translateDdlWarning';
import { WarningsBanner } from '@/components/Table/WarningsBanner';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { Driver } from '@/lib/connection/drivers';
import type { DdlWarning } from '@/lib/ddl';
import { loadBaselineFile, saveBaseline, useBaseline } from '@/lib/migrations/baselineStore';
import { NON_TX_DDL_DRIVERS, SCHEMA_MIGRATION_DRIVERS } from '@/lib/migrations/drivers';
import { loadMigrations, useMigrationsStore } from '@/lib/migrations/migrationsStore';
import {
  buildMigrationFilename,
  nextVersion,
  parseMigration,
  serializeMigration,
  slugify,
  summarize,
} from '@/lib/migrations/parse';
import { compareSnapshots, type SchemaDelta } from '@/lib/migrations/schemaCompare';
import { captureSnapshot, generateMigration } from '@/lib/migrations/schemaDiff';
import { nextMigrationDirection } from '@/lib/migrations/status';
import { notify } from '@/lib/notify';
import { confirmDialog } from '@/lib/stores/confirmStore';
import {
  applyMigration,
  getMigrationStatus,
  listNamespaces,
  type MigrationDirection,
  type MigrationStatusEntry,
  wsDeleteMigration,
  wsReadMigration,
  wsWriteMigration,
} from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { useLicense } from '@/providers/LicenseProvider';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { SchemaDeltaView } from './SchemaDeltaView';

function prependWarnings(up: string, warnings: DdlWarning[], t: TFunction): string {
  if (warnings.length === 0) return up;
  const lines = warnings.map(w => `-- WARNING: ${translateDdlWarning(t, w)}`);
  return `${lines.join('\n')}\n${up}`;
}

interface MigrationsPanelProps {
  sessionId?: string;
  connectionId?: string;
  database?: string;
  driver?: string;
  environment?: string;
  readOnly?: boolean;
}

export function MigrationsPanel({
  sessionId,
  connectionId,
  database,
  driver,
  environment,
  readOnly,
}: MigrationsPanelProps) {
  const { t } = useTranslation();
  const { activeWorkspace, projectId } = useWorkspace();
  const { isFeatureEnabled } = useLicense();
  const migrations = useMigrationsStore(s => s.migrations);
  const isLoading = useMigrationsStore(s => s.isLoading);

  const [newName, setNewName] = useState('');
  const [creating, setCreating] = useState(false);
  const [selected, setSelected] = useState<string | null>(null);
  const [detail, setDetail] = useState<{ up: string; down: string } | null>(null);
  const [statusByVersion, setStatusByVersion] = useState<Record<string, MigrationStatusEntry>>({});
  const [applying, setApplying] = useState<string | null>(null);
  const [capturing, setCapturing] = useState(false);
  const [generating, setGenerating] = useState(false);
  const [checkingDrift, setCheckingDrift] = useState(false);
  const [driftReport, setDriftReport] = useState<SchemaDelta | null>(null);
  const [generateWarnings, setGenerateWarnings] = useState<DdlWarning[]>([]);
  const [draft, setDraft] = useState<{ up: string; down: string } | null>(null);
  const [saving, setSaving] = useState(false);
  const [targetDatabase, setTargetDatabase] = useState(database?.trim() ?? '');
  const [databaseOptions, setDatabaseOptions] = useState<string[]>([]);
  const [mysqlWarningVisible, setMysqlWarningVisible] = useState(true);

  const isDefault = activeWorkspace == null || activeWorkspace.source === 'default';
  const driverSupported = driver != null && SCHEMA_MIGRATION_DRIVERS.has(driver);
  const requiresTargetDatabase = driver === 'mysql' || driver === 'mariadb';
  const databaseReady = !requiresTargetDatabase || targetDatabase.length > 0;
  const canApply = !!sessionId && driverSupported && !readOnly && databaseReady;

  const schemaDiffAvailable = !!sessionId && !!connectionId && driverSupported && databaseReady;
  const hasSchemaDiff = isFeatureEnabled('schema_diff');
  const canGenerate = schemaDiffAvailable && hasSchemaDiff;
  const baseline = useBaseline(connectionId ?? null, targetDatabase || undefined);

  useEffect(() => {
    setTargetDatabase(database?.trim() ?? '');
  }, [database]);

  useEffect(() => {
    if (!sessionId || !driverSupported) {
      setDatabaseOptions([]);
      return;
    }
    let cancelled = false;
    void listNamespaces(sessionId)
      .then(result => {
        if (cancelled || !result.success) return;
        const databases = Array.from(
          new Set((result.namespaces ?? []).map(namespace => namespace.database).filter(Boolean))
        ).sort((a, b) => a.localeCompare(b));
        setDatabaseOptions(databases);
        if (databases.length === 1) setTargetDatabase(current => current || databases[0]);
      })
      .catch(() => {
        if (!cancelled) setDatabaseOptions([]);
      });
    return () => {
      cancelled = true;
    };
  }, [driverSupported, sessionId]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `projectId` is intentional — reload migrations when switching between file-based workspaces (isDefault stays false)
  useEffect(() => {
    if (!isDefault) void loadMigrations();
  }, [isDefault, projectId]);

  useEffect(() => {
    if (connectionId) void loadBaselineFile(connectionId);
  }, [connectionId]);

  const refreshStatus = useCallback(async () => {
    if (!sessionId) {
      setStatusByVersion({});
      return;
    }
    try {
      if (requiresTargetDatabase && !targetDatabase) {
        setStatusByVersion({});
        return;
      }
      const entries = await getMigrationStatus(sessionId, targetDatabase || undefined);
      setStatusByVersion(entries ? Object.fromEntries(entries.map(e => [e.version, e])) : {});
    } catch {
      setStatusByVersion({});
    }
  }, [requiresTargetDatabase, sessionId, targetDatabase]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `migrations` is intentional — refresh applied status (and checksum drift) when the file list reloads
  useEffect(() => {
    void refreshStatus();
  }, [refreshStatus, migrations]);

  // biome-ignore lint/correctness/useExhaustiveDependencies: `migrations` is intentional — re-read the open file when the list reloads after an external edit
  useEffect(() => {
    if (!selected) {
      setDetail(null);
      setDraft(null);
      return;
    }
    let cancelled = false;
    wsReadMigration(selected)
      .then(content => {
        if (cancelled) return;
        const parsed = content == null ? null : parseMigration(content);
        setDetail(parsed);
        setDraft(parsed);
      })
      .catch(() => {
        if (cancelled) return;
        setDetail(null);
        setDraft(null);
      });
    return () => {
      cancelled = true;
    };
  }, [selected, migrations]);

  const isDirty =
    draft != null && detail != null && (draft.up !== detail.up || draft.down !== detail.down);

  const handleSave = useCallback(async () => {
    if (!selected || !draft) return;
    setSaving(true);
    try {
      const ok = await wsWriteMigration(selected, serializeMigration(draft.up, draft.down));
      if (!ok) {
        notify.error(t('migrations.requiresWorkspace'));
        return;
      }
      setDetail(draft);
      await refreshStatus();
      notify.success(t('migrations.saved'));
    } catch (err) {
      notify.error(t('common.unknownError'), err);
    } finally {
      setSaving(false);
    }
  }, [selected, draft, refreshStatus, t]);

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
      const version = summarize(filename).version;

      const applied = statusByVersion[version]?.status === 'applied';
      const confirmed = await confirmDialog({
        title: t('migrations.delete'),
        description: t('migrations.deleteConfirm', { name: filename }),
        warningInfo: applied ? t('migrations.deleteAppliedWarning') : undefined,
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
    [selected, statusByVersion, t]
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
        let res = await applyMigration(sessionId, filename, direction, targetDatabase, true);

        // Some refusals are judgement calls rather than errors: the file drifted,
        // or a previous run died and left the schema in an unknown state. Surface
        // the reason and let the user decide.
        if (!res.success && res.overridable) {
          const partial = res.blocked_reason === 'partially_applied';
          const forced = await confirmDialog({
            title: partial ? t('migrations.partiallyApplied') : t('migrations.checksumMismatch'),
            description: res.error ?? t('migrations.checksumForceConfirm', { name: filename }),
            warningInfo: partial
              ? t('migrations.partiallyAppliedWarning')
              : t('migrations.checksumForceWarning'),
            confirmLabel: t('migrations.forceApply'),
          });
          if (!forced) {
            await refreshStatus();
            return;
          }
          res = await applyMigration(sessionId, filename, direction, targetDatabase, true, true);
        }

        if (res.success) {
          notify.success(isUp ? t('migrations.applied') : t('migrations.rolledBack'));
        } else {
          notify.error(res.error ?? t('common.unknownError'));
        }
        // A script error can mark a non-transactional run as `failed` without a
        // blocked_reason. Always refresh so the badge and next direction reflect
        // the database history rather than the pre-run cache.
        await refreshStatus();
      } catch (err) {
        notify.error(t('common.unknownError'));
        console.error('Failed to apply migration:', err);
        // The backend may have completed even if the response transport failed.
        await refreshStatus();
      } finally {
        setApplying(null);
      }
    },
    [sessionId, targetDatabase, environment, refreshStatus, t]
  );

  const handleCaptureBaseline = useCallback(async () => {
    if (!sessionId || !connectionId || !driverSupported) return;
    setCapturing(true);
    try {
      const { snapshot, failedTables } = await captureSnapshot(
        sessionId,
        driver as Driver,
        targetDatabase || undefined
      );
      await saveBaseline(connectionId, targetDatabase || undefined, snapshot);
      if (failedTables.length > 0) {
        notify.warning(t('migrations.captureIncomplete', { count: failedTables.length }));
      } else {
        notify.success(t('migrations.baselineCaptured'));
      }
    } catch (err) {
      notify.error(t('common.unknownError'), err);
    } finally {
      setCapturing(false);
    }
  }, [sessionId, connectionId, driverSupported, driver, targetDatabase, t]);

  const handleGenerate = useCallback(async () => {
    if (!sessionId || !connectionId || !driverSupported || !baseline) return;
    setGenerating(true);
    try {
      const { snapshot: live, failedTables } = await captureSnapshot(
        sessionId,
        driver as Driver,
        targetDatabase || undefined
      );
      // An incomplete capture would make missing tables look dropped and emit
      // destructive DROP statements — refuse to generate from a partial snapshot.
      if (failedTables.length > 0) {
        notify.error(t('migrations.generateIncomplete', { count: failedTables.length }));
        return;
      }
      const result = generateMigration(baseline, live, driver as Driver);
      setGenerateWarnings(result.warnings);
      if (result.isEmpty) {
        notify.info(t('migrations.noChanges'));
        return;
      }
      // The schemas differ but this dialect can't express the change. Emitting
      // the empty script would read as "no changes" — refuse and say why.
      if (result.unexpressed.length > 0) {
        notify.error(
          t('migrations.generateUnexpressed', {
            count: result.unexpressed.length,
          })
        );
        return;
      }
      const filename = buildMigrationFilename(
        nextVersion(migrations ?? []),
        slugify('schema_changes')
      );
      const body = serializeMigration(prependWarnings(result.up, result.warnings, t), result.down);
      const ok = await wsWriteMigration(filename, body);
      if (!ok) {
        notify.error(t('migrations.requiresWorkspace'));
        return;
      }
      await loadMigrations();
      setSelected(filename);
      setDriftReport(null);
      // Adopt the live schema as the new baseline so the same delta isn't re-emitted.
      await saveBaseline(connectionId, targetDatabase || undefined, live);
      if (result.hasIrreversible) {
        notify.warning(t('migrations.generatedIrreversible'));
      } else {
        notify.success(t('migrations.generated'));
      }
    } catch (err) {
      notify.error(t('common.unknownError'), err);
    } finally {
      setGenerating(false);
    }
  }, [sessionId, connectionId, driverSupported, driver, targetDatabase, baseline, migrations, t]);

  const handleCheckDrift = useCallback(async () => {
    if (!sessionId || !driverSupported || !baseline) return;
    setCheckingDrift(true);
    try {
      const { snapshot: live, failedTables } = await captureSnapshot(
        sessionId,
        driver as Driver,
        targetDatabase || undefined
      );
      if (failedTables.length > 0) {
        notify.warning(t('migrations.captureIncomplete', { count: failedTables.length }));
      }
      // Skip tables that failed to describe so a transient failure isn't reported as a drop.
      const delta = compareSnapshots(baseline, live, {
        ignoreKeys: new Set(failedTables),
      });
      setSelected(null);
      setDriftReport(delta);
      if (!delta.hasChanges) notify.success(t('migrations.driftNone'));
    } catch (err) {
      notify.error(t('common.unknownError'), err);
    } finally {
      setCheckingDrift(false);
    }
  }, [sessionId, driverSupported, driver, targetDatabase, baseline, t]);

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
  const selectedMigration = list.find(migration => migration.filename === selected);
  const selectedStatus = selectedMigration ? statusByVersion[selectedMigration.version] : undefined;
  const selectedDirection = nextMigrationDirection(selectedStatus);

  return (
    <div className="flex h-full min-h-0 flex-col p-3">
      <div className="flex-1 min-h-0 flex flex-col overflow-hidden rounded-xl border border-border bg-background shadow-sm">
        <div className="border-b border-border">
          <div className="flex flex-wrap items-center justify-between gap-4 px-5 pt-4 pb-3">
            <div className="min-w-0">
              <h2 className="text-base font-semibold">{t('migrations.title')}</h2>
              <p className="mt-0.5 text-xs text-muted-foreground">
                {!sessionId
                  ? t('migrations.connectHint')
                  : !driverSupported
                    ? t('migrations.driverUnsupported')
                    : t('migrations.description')}
              </p>
            </div>

            <div className="flex items-center gap-2">
              {sessionId && driverSupported && databaseOptions.length > 0 && (
                <>
                  <span className="text-xs font-medium text-muted-foreground">
                    {t('migrations.targetDatabase')}
                  </span>
                  <Select value={targetDatabase || undefined} onValueChange={setTargetDatabase}>
                    <SelectTrigger
                      size="sm"
                      className="min-w-28 max-w-64"
                      aria-label={t('migrations.targetDatabase')}
                    >
                      <Database className="size-3.5" />
                      <SelectValue placeholder={t('migrations.selectDatabase')} />
                    </SelectTrigger>
                    <SelectContent>
                      {databaseOptions.map(option => (
                        <SelectItem key={option} value={option}>
                          {option}
                        </SelectItem>
                      ))}
                    </SelectContent>
                  </Select>
                </>
              )}
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  void loadMigrations();
                  void refreshStatus();
                }}
                title={t('migrations.refresh')}
                aria-label={t('migrations.refresh')}
              >
                <RefreshCw className={cn('size-4', isLoading && 'animate-spin')} />
              </Button>
            </div>
          </div>

          <div className="flex flex-wrap items-center justify-between gap-3 px-5 pb-4">
            <div className="flex items-center gap-2">
              {canGenerate && (
                <>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleCaptureBaseline()}
                    disabled={capturing}
                    title={t('migrations.captureBaselineHint')}
                  >
                    {capturing ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <Camera className="size-4" />
                    )}
                    {baseline ? t('migrations.recaptureBaseline') : t('migrations.captureBaseline')}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleGenerate()}
                    disabled={!baseline || generating}
                    title={t('migrations.generateHint')}
                  >
                    {generating ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <Wand2 className="size-4" />
                    )}
                    {t('migrations.generate')}
                  </Button>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={() => void handleCheckDrift()}
                    disabled={!baseline || checkingDrift}
                    title={t('migrations.driftCheckHint')}
                  >
                    {checkingDrift ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <ScanSearch className="size-4" />
                    )}
                    {t('migrations.driftCheck')}
                  </Button>
                </>
              )}
            </div>

            {list.length > 0 && (
              <div className="flex items-center gap-2">
                <Input
                  value={newName}
                  onChange={event => setNewName(event.target.value)}
                  onKeyDown={event => {
                    if (event.key === 'Enter') void handleCreate();
                  }}
                  placeholder={t('migrations.newPlaceholder')}
                  className="h-8 w-60"
                />
                <Button
                  onClick={() => void handleCreate()}
                  disabled={creating || !newName.trim()}
                  size="sm"
                >
                  {creating ? (
                    <Loader2 className="size-4 animate-spin" />
                  ) : (
                    <Plus className="size-4" />
                  )}
                  {t('migrations.create')}
                </Button>
              </div>
            )}
          </div>
        </div>

        {schemaDiffAvailable && !hasSchemaDiff && (
          <div className="px-4 py-2 border-b border-border">
            <UpgradePrompt
              feature="schema_diff"
              variant="compact"
              source="migrations"
              hideIfDismissed
            />
          </div>
        )}

        {canGenerate && baseline && (
          <div className="flex items-center gap-2 px-4 py-2 text-xs bg-muted/40 text-muted-foreground border-b border-border">
            <Camera className="w-3.5 h-3.5 shrink-0" />
            {t('migrations.baselineInfo', {
              time: new Date(baseline.capturedAt).toLocaleTimeString(),
              count: Object.keys(baseline.tables).length,
            })}
          </div>
        )}

        {sessionId && driverSupported && requiresTargetDatabase && !targetDatabase && (
          <div className="flex items-center gap-2 px-4 py-2 text-xs bg-destructive/10 text-destructive border-b border-border">
            <Database className="w-3.5 h-3.5 shrink-0" />
            {t('migrations.databaseRequired')}
          </div>
        )}

        {sessionId &&
          driverSupported &&
          !readOnly &&
          mysqlWarningVisible &&
          NON_TX_DDL_DRIVERS.has(driver ?? '') && (
            <div className="flex items-center gap-2 px-4 py-2 text-xs bg-amber-500/10 text-amber-700 dark:text-amber-400 border-b border-border">
              <AlertTriangle className="w-3.5 h-3.5 shrink-0" />
              <span className="flex-1">{t('migrations.mysqlCaveat')}</span>
              <button
                type="button"
                onClick={() => setMysqlWarningVisible(false)}
                aria-label={t('migrations.close')}
                title={t('migrations.close')}
                className="rounded-md p-1 text-amber-700/70 transition-colors hover:bg-amber-500/15 hover:text-amber-800 dark:text-amber-400/70 dark:hover:text-amber-300"
              >
                <X className="size-3.5" />
              </button>
            </div>
          )}

        <div className="flex min-h-0 flex-1 overflow-hidden">
          {list.length === 0 ? (
            <div className="flex-1 flex items-center justify-center p-8">
              <div className="w-full max-w-lg rounded-xl border border-border bg-muted/20 p-8 text-center shadow-sm">
                <div className="mx-auto mb-4 flex size-11 items-center justify-center rounded-xl bg-accent/10 text-accent">
                  <FileCode className="size-5" />
                </div>
                <h3 className="text-base font-semibold">{t('migrations.empty')}</h3>
                <p className="mt-1 text-sm text-muted-foreground">{t('migrations.emptyHint')}</p>
                <div className="mx-auto mt-5 flex max-w-md items-center gap-2">
                  <Input
                    value={newName}
                    onChange={event => setNewName(event.target.value)}
                    onKeyDown={event => {
                      if (event.key === 'Enter') void handleCreate();
                    }}
                    placeholder={t('migrations.newPlaceholder')}
                    autoFocus
                  />
                  <Button
                    onClick={() => void handleCreate()}
                    disabled={creating || !newName.trim()}
                    className="shrink-0"
                  >
                    {creating ? (
                      <Loader2 className="size-4 animate-spin" />
                    ) : (
                      <Plus className="size-4" />
                    )}
                    {t('migrations.create')}
                  </Button>
                </div>
              </div>
            </div>
          ) : (
            <>
              <div className="w-80 shrink-0 border-r border-border overflow-y-auto p-2">
                {list.map(m => {
                  const status = statusByVersion[m.version];
                  const direction = nextMigrationDirection(status);
                  const rollingBack = direction === 'down';
                  const busy = applying === m.filename;
                  return (
                    <div
                      key={m.filename}
                      className={cn(
                        'flex items-center rounded-lg border border-transparent',
                        selected === m.filename ? 'border-border bg-muted' : 'hover:bg-muted/50'
                      )}
                    >
                      <button
                        type="button"
                        onClick={() => {
                          setSelected(m.filename);
                          setDriftReport(null);
                        }}
                        className="flex-1 min-w-0 text-left px-3 py-2 flex items-center gap-2 rounded-l-lg"
                      >
                        <span className="font-mono text-xs text-muted-foreground">{m.version}</span>
                        <span className="flex-1 min-w-0 truncate text-sm">{m.name}</span>
                        {status && <StatusBadge status={status.status} />}
                        {status?.checksum_mismatch && (
                          <span title={t('migrations.checksumMismatch')} className="shrink-0">
                            <AlertTriangle className="w-3.5 h-3.5 text-amber-500" />
                          </span>
                        )}
                        {(status?.duplicate_version || status?.malformed) && (
                          <span
                            title={
                              status.duplicate_version
                                ? t('migrations.duplicateVersion')
                                : t('migrations.malformed')
                            }
                            className="shrink-0"
                          >
                            <AlertTriangle className="w-3.5 h-3.5 text-destructive" />
                          </span>
                        )}
                      </button>
                      {canApply && (
                        <button
                          type="button"
                          onClick={() => void handleApply(m.filename, direction)}
                          disabled={applying !== null || (selected === m.filename && isDirty)}
                          title={rollingBack ? t('migrations.rollback') : t('migrations.apply')}
                          className="px-2 py-2 rounded-md text-muted-foreground hover:bg-background hover:text-foreground disabled:opacity-50"
                        >
                          {busy ? (
                            <Loader2 className="w-3.5 h-3.5 animate-spin" />
                          ) : rollingBack ? (
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
                        className="px-2 py-2 rounded-md text-muted-foreground hover:bg-background hover:text-destructive"
                      >
                        <Trash2 className="w-3.5 h-3.5" />
                      </button>
                    </div>
                  );
                })}
              </div>

              <div className="min-h-0 min-w-0 flex-1 overflow-y-auto p-4 pb-8">
                {generateWarnings.length > 0 && (
                  <div className="mb-3">
                    <WarningsBanner warnings={generateWarnings} defaultOpen />
                  </div>
                )}
                {driftReport ? (
                  <div className="flex flex-col gap-3">
                    <div className="flex items-center justify-between">
                      <h3 className="text-sm font-semibold">{t('migrations.driftTitle')}</h3>
                      <button
                        type="button"
                        onClick={() => setDriftReport(null)}
                        title={t('migrations.close')}
                        className="text-muted-foreground hover:text-foreground"
                      >
                        <X className="w-4 h-4" />
                      </button>
                    </div>
                    <SchemaDeltaView delta={driftReport} />
                  </div>
                ) : draft == null ? (
                  <div className="h-full flex items-center justify-center text-sm text-muted-foreground">
                    {t('migrations.selectHint')}
                  </div>
                ) : (
                  <div className="flex flex-col gap-4">
                    <div className="flex items-center justify-between gap-2">
                      <div className="min-w-0">
                        <h3 className="truncate text-sm font-semibold">
                          {selectedMigration?.name}
                        </h3>
                        <p className="truncate font-mono text-xs text-muted-foreground">
                          {selectedMigration?.filename}
                        </p>
                      </div>
                      <div className="flex items-center gap-2">
                        {isDirty && (
                          <span className="text-xs text-muted-foreground">
                            {t('migrations.unsavedChanges')}
                          </span>
                        )}
                        <Button
                          size="sm"
                          variant="outline"
                          onClick={() => void handleSave()}
                          disabled={!isDirty || saving}
                        >
                          {saving ? (
                            <Loader2 className="w-3.5 h-3.5 mr-1.5 animate-spin" />
                          ) : (
                            <Save className="w-3.5 h-3.5 mr-1.5" />
                          )}
                          {t('common.save')}
                        </Button>
                        <Button
                          size="sm"
                          onClick={() => {
                            if (selected) void handleApply(selected, selectedDirection);
                          }}
                          disabled={!canApply || !selected || applying !== null || isDirty}
                          title={isDirty ? t('migrations.saveBeforeApply') : undefined}
                        >
                          {applying === selected ? (
                            <Loader2 className="size-3.5 animate-spin" />
                          ) : selectedDirection === 'down' ? (
                            <Undo2 className="size-3.5" />
                          ) : (
                            <Play className="size-3.5" />
                          )}
                          {selectedDirection === 'down'
                            ? t('migrations.rollback')
                            : t('migrations.apply')}
                        </Button>
                      </div>
                    </div>
                    <section>
                      <div className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                        {t('migrations.upLabel')}
                      </div>
                      {/* `readOnly` guards the connection, not the workspace: editing a
                    local migration file writes no SQL anywhere. */}
                      <div className="h-56 rounded-lg border border-border overflow-hidden">
                        <SQLEditor
                          value={draft.up}
                          onChange={up => setDraft(d => (d ? { ...d, up } : d))}
                          dialect={driver as Driver}
                          sessionId={sessionId}
                          connectionDatabase={targetDatabase || undefined}
                          placeholder={t('migrations.upPlaceholder')}
                        />
                      </div>
                    </section>
                    <section>
                      <div className="text-xs font-semibold uppercase text-muted-foreground mb-1">
                        {t('migrations.downLabel')}
                      </div>
                      <div className="h-56 rounded-lg border border-border overflow-hidden">
                        <SQLEditor
                          value={draft.down}
                          onChange={down => setDraft(d => (d ? { ...d, down } : d))}
                          dialect={driver as Driver}
                          sessionId={sessionId}
                          connectionDatabase={targetDatabase || undefined}
                          placeholder={t('migrations.downPlaceholder')}
                        />
                      </div>
                    </section>
                  </div>
                )}
              </div>
            </>
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
    failed: 'bg-destructive/15 text-destructive',
  };
  const labels: Record<MigrationStatusEntry['status'], string> = {
    applied: t('migrations.statusApplied'),
    pending: t('migrations.statusPending'),
    rolled_back: t('migrations.statusRolledBack'),
    failed: t('migrations.statusFailed'),
  };
  return (
    <span className={cn('shrink-0 rounded px-1.5 py-0.5 text-[10px] font-medium', styles[status])}>
      {labels[status]}
    </span>
  );
}

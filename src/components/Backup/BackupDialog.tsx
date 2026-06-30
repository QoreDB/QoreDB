// SPDX-License-Identifier: Apache-2.0

import { open as openDialog, save } from '@tauri-apps/plugin-dialog';
import { Loader2, Play, X } from 'lucide-react';
import { useCallback, useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { Label } from '@/components/ui/label';
import { ScrollArea } from '@/components/ui/scroll-area';
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from '@/components/ui/select';
import type { SavedConnection } from '@/lib/tauri';
import {
  type BackupEvent,
  type BackupFormat,
  type BackupMode,
  type BackupOptions,
  cancelBackup,
  listenBackupProgress,
  startBackup,
} from '@/lib/tauri/backup';

interface BackupDialogProps {
  connection: SavedConnection | null;
  database: string | null;
  open: boolean;
  onClose: () => void;
}

type Phase = 'idle' | 'running' | 'done';

const PG_DRIVERS = new Set([
  'postgres',
  'postgresql',
  'supabase',
  'neon',
  'timescaledb',
  'cockroachdb',
]);

function defaultExtension(driver: string, format: BackupFormat): string {
  if (driver === 'mongodb') return 'archive';
  if (format === 'postgres_custom') return 'dump';
  return 'sql';
}

export function BackupDialog({ connection, database, open, onClose }: BackupDialogProps) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<BackupMode>('full');
  const [format, setFormat] = useState<BackupFormat>('sql');
  const [password, setPassword] = useState('');
  const [tablesRaw, setTablesRaw] = useState('');
  const [outputPath, setOutputPath] = useState('');
  const [phase, setPhase] = useState<Phase>('idle');
  const [logs, setLogs] = useState<{ id: number; line: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const logEndRef = useRef<HTMLDivElement | null>(null);
  const logCounter = useRef(0);

  // Reset on open
  useEffect(() => {
    if (open) {
      setMode('full');
      setFormat('sql');
      setPassword('');
      setTablesRaw('');
      setOutputPath('');
      setPhase('idle');
      setLogs([]);
      setError(null);
      setJobId(null);
      setCancelling(false);
    }
  }, [open]);

  // Auto-scroll logs
  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  const driver = connection?.driver ?? 'postgres';
  const isPostgres = PG_DRIVERS.has(driver);
  const isDuckdb = driver === 'duckdb';
  // File-based engines store a path in `database`, unfit for a title.
  const titleName =
    driver === 'sqlite' || isDuckdb ? connection?.name : (database ?? connection?.name);

  const pickOutput = useCallback(async () => {
    if (!connection) return;
    if (isDuckdb) {
      // DuckDB EXPORT DATABASE writes a directory, not a single file.
      const chosen = await openDialog({ directory: true, multiple: false });
      if (typeof chosen === 'string') setOutputPath(chosen);
      return;
    }
    const ext = defaultExtension(driver, format);
    const suggested = `${database ?? connection.name}-${new Date().toISOString().slice(0, 10)}.${ext}`;
    const chosen = await save({
      defaultPath: suggested,
      filters: [{ name: ext.toUpperCase(), extensions: [ext] }],
    });
    if (chosen) setOutputPath(chosen);
  }, [connection, database, driver, format, isDuckdb]);

  const handleStart = useCallback(async () => {
    if (!connection || !outputPath) return;

    setPhase('running');
    setLogs([]);
    setError(null);

    const tables = tablesRaw
      .split(/[\s,]+/)
      .map(s => s.trim())
      .filter(Boolean);

    const options: BackupOptions = {
      driver,
      mode,
      format,
      host: connection.host,
      port: connection.port,
      username: connection.username || null,
      password: password || null,
      database: database ?? null,
      tables,
      output_path: outputPath,
    };

    let unlistenAll: (() => void) | null = null;
    try {
      const { listen } = await import('@tauri-apps/api/event');
      unlistenAll = await listen<{ job_id: string; event: BackupEvent }>('backup-progress', evt => {
        const event = evt.payload.event;
        if (event.kind === 'started') {
          setJobId(event.job_id);
        } else if (event.kind === 'log') {
          const id = ++logCounter.current;
          setLogs(prev => [...prev, { id, line: `[${event.stream}] ${event.line}` }]);
        }
      });

      const outcome = await startBackup(options);
      if (unlistenAll) unlistenAll();

      const finalUnlisten = await listenBackupProgress(outcome.job_id, () => {});
      finalUnlisten();

      setPhase('done');
      setJobId(null);
      setCancelling(false);
      if (!outcome.success) {
        setError(t('backup.exitedWithCode', { code: outcome.exit_code ?? 'signal' }));
      }
    } catch (e) {
      if (unlistenAll) unlistenAll();
      setPhase('done');
      setJobId(null);
      setCancelling(false);
      setError(e instanceof Error ? e.message : String(e));
    }
  }, [connection, database, driver, format, mode, outputPath, password, tablesRaw, t]);

  const handleCancel = useCallback(async () => {
    if (!jobId || cancelling) return;
    setCancelling(true);
    try {
      await cancelBackup(jobId);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setCancelling(false);
    }
  }, [jobId, cancelling]);

  if (!connection) return null;

  return (
    <Dialog open={open} onOpenChange={o => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('backup.dialogTitle', { name: titleName })}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="grid grid-cols-2 gap-3">
            <div>
              <Label className="text-xs text-muted-foreground">{t('backup.mode')}</Label>
              <Select
                value={mode}
                onValueChange={v => setMode(v as BackupMode)}
                disabled={phase !== 'idle'}
              >
                <SelectTrigger>
                  <SelectValue />
                </SelectTrigger>
                <SelectContent>
                  <SelectItem value="full">{t('backup.modeFull')}</SelectItem>
                  <SelectItem value="schema_only">{t('backup.modeSchemaOnly')}</SelectItem>
                  <SelectItem value="data_only">{t('backup.modeDataOnly')}</SelectItem>
                </SelectContent>
              </Select>
            </div>

            {isPostgres && (
              <div>
                <Label className="text-xs text-muted-foreground">{t('backup.format')}</Label>
                <Select
                  value={format}
                  onValueChange={v => setFormat(v as BackupFormat)}
                  disabled={phase !== 'idle'}
                >
                  <SelectTrigger>
                    <SelectValue />
                  </SelectTrigger>
                  <SelectContent>
                    <SelectItem value="sql">SQL plain</SelectItem>
                    <SelectItem value="postgres_custom">pg_dump custom</SelectItem>
                  </SelectContent>
                </Select>
              </div>
            )}
          </div>

          {driver !== 'sqlite' && (
            <div>
              <Label className="text-xs text-muted-foreground">{t('backup.password')}</Label>
              <Input
                type="password"
                value={password}
                onChange={e => setPassword(e.target.value)}
                disabled={phase !== 'idle'}
                placeholder={t('backup.passwordPlaceholder')}
              />
            </div>
          )}

          <div>
            <Label className="text-xs text-muted-foreground">{t('backup.tablesOptional')}</Label>
            <Input
              value={tablesRaw}
              onChange={e => setTablesRaw(e.target.value)}
              disabled={phase !== 'idle'}
              placeholder={t('backup.tablesPlaceholder')}
            />
          </div>

          <div>
            <Label className="text-xs text-muted-foreground">
              {isDuckdb ? t('backup.outputDir') : t('backup.outputPath')}
            </Label>
            <div className="flex items-center gap-2">
              <Input
                value={outputPath}
                readOnly
                placeholder={t('backup.outputPathPlaceholder')}
                className="flex-1"
              />
              <Button variant="outline" onClick={pickOutput} disabled={phase !== 'idle'}>
                {t('backup.browse')}
              </Button>
            </div>
            {isDuckdb && (
              <p className="text-[11px] text-muted-foreground mt-1">{t('backup.duckdbHint')}</p>
            )}
          </div>

          {(phase === 'running' || phase === 'done') && (
            <div className="space-y-1">
              <Label className="text-xs text-muted-foreground">{t('backup.progress')}</Label>
              <ScrollArea className="h-48 rounded border border-border bg-muted/30 p-2 font-mono text-xs">
                {logs.length === 0 && phase === 'running' && (
                  <p className="text-muted-foreground">{t('backup.starting')}</p>
                )}
                {logs.map(entry => (
                  <div key={entry.id} className="whitespace-pre-wrap break-all">
                    {entry.line}
                  </div>
                ))}
                <div ref={logEndRef} />
              </ScrollArea>
            </div>
          )}

          {error && <p className="text-sm text-destructive">{error}</p>}

          {phase === 'done' && !error && (
            <p className="text-sm text-green-500">{t('backup.completed')}</p>
          )}
        </div>

        <DialogFooter>
          {phase === 'idle' && (
            <>
              <Button variant="outline" onClick={onClose}>
                {t('common.cancel')}
              </Button>
              <Button onClick={handleStart} disabled={!outputPath}>
                <Play size={14} className="mr-1" />
                {t('backup.start')}
              </Button>
            </>
          )}
          {phase === 'running' && (
            <>
              <Button disabled variant="outline">
                <Loader2 size={14} className="mr-1 animate-spin" />
                {t('backup.running')}
              </Button>
              <Button variant="destructive" onClick={handleCancel} disabled={!jobId || cancelling}>
                <X size={14} className="mr-1" />
                {cancelling ? t('backup.cancelling') : t('backup.cancel')}
              </Button>
            </>
          )}
          {phase === 'done' && <Button onClick={onClose}>{t('common.close')}</Button>}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

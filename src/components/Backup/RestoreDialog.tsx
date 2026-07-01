// SPDX-License-Identifier: Apache-2.0

import { open } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, Loader2, Upload, X } from 'lucide-react';
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
  cancelBackup,
  type RestoreOptions,
  startRestore,
} from '@/lib/tauri/backup';

interface RestoreDialogProps {
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

function defaultExtensionsForDriver(driver: string): string[] {
  if (PG_DRIVERS.has(driver)) return ['sql', 'dump', 'tar'];
  if (driver === 'mongodb') return ['archive', 'gz'];
  if (driver === 'sqlite') return ['sql', 'sqlite'];
  return ['sql'];
}

export function RestoreDialog({ connection, database, open: isOpen, onClose }: RestoreDialogProps) {
  const { t } = useTranslation();
  const [format, setFormat] = useState<BackupFormat>('sql');
  const [password, setPassword] = useState('');
  const [inputPath, setInputPath] = useState('');
  const [confirmDb, setConfirmDb] = useState('');
  const [phase, setPhase] = useState<Phase>('idle');
  const [logs, setLogs] = useState<{ id: number; line: string }[]>([]);
  const [error, setError] = useState<string | null>(null);
  const [jobId, setJobId] = useState<string | null>(null);
  const [cancelling, setCancelling] = useState(false);
  const logEndRef = useRef<HTMLDivElement | null>(null);
  const logCounter = useRef(0);

  useEffect(() => {
    if (isOpen) {
      setFormat('sql');
      setPassword('');
      setInputPath('');
      setConfirmDb('');
      setPhase('idle');
      setLogs([]);
      setError(null);
      setJobId(null);
      setCancelling(false);
      logCounter.current = 0;
    }
  }, [isOpen]);

  useEffect(() => {
    logEndRef.current?.scrollIntoView({ behavior: 'smooth' });
  }, []);

  const driver = connection?.driver ?? 'postgres';
  const isPostgres = PG_DRIVERS.has(driver);
  const isDuckdb = driver === 'duckdb';
  const expectedDb = database ?? '';
  // File-based engines store a path in `database`, unfit for a title.
  const titleName =
    driver === 'sqlite' || isDuckdb ? connection?.name : (database ?? connection?.name);
  // Confirmation only required when actually targeting a named database
  const requiresConfirm = !!expectedDb;
  const confirmOk = !requiresConfirm || confirmDb.trim() === expectedDb;

  const pickInput = useCallback(async () => {
    if (isDuckdb) {
      // DuckDB IMPORT DATABASE reads a directory produced by EXPORT DATABASE.
      const chosen = await open({ directory: true, multiple: false });
      if (typeof chosen === 'string') setInputPath(chosen);
      return;
    }
    const exts = defaultExtensionsForDriver(driver);
    const chosen = await open({
      multiple: false,
      filters: [{ name: 'Backup file', extensions: exts }],
    });
    if (typeof chosen === 'string') setInputPath(chosen);
  }, [driver, isDuckdb]);

  const handleStart = useCallback(async () => {
    if (!connection || !inputPath || !confirmOk) return;

    setPhase('running');
    setLogs([]);
    setError(null);

    const options: RestoreOptions = {
      driver,
      host: connection.host,
      port: connection.port,
      username: connection.username || null,
      password: password || null,
      database: database ?? null,
      input_path: inputPath,
      format,
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

      const outcome = await startRestore(options, connection.id, connection.project_id);
      if (unlistenAll) unlistenAll();

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
  }, [connection, database, confirmOk, driver, format, inputPath, password, t]);

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
    <Dialog open={isOpen} onOpenChange={o => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('restore.dialogTitle', { name: titleName })}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="flex items-start gap-2 p-3 rounded-md bg-warning/10 border border-warning/40 text-xs">
            <AlertTriangle size={14} className="text-warning shrink-0 mt-0.5" />
            <span className="text-warning-foreground">
              {t('restore.destructiveWarning', { name: expectedDb || connection.name })}
            </span>
          </div>

          <div>
            <Label className="text-xs text-muted-foreground">
              {isDuckdb ? t('backup.outputDir') : t('restore.inputPath')}
            </Label>
            <div className="flex items-center gap-2">
              <Input
                value={inputPath}
                readOnly
                placeholder={t('restore.inputPathPlaceholder')}
                className="flex-1"
              />
              <Button variant="outline" onClick={pickInput} disabled={phase !== 'idle'}>
                {t('backup.browse')}
              </Button>
            </div>
            {isDuckdb && (
              <p className="text-[11px] text-muted-foreground mt-1">{t('restore.duckdbHint')}</p>
            )}
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
                  <SelectItem value="sql">SQL plain (psql)</SelectItem>
                  <SelectItem value="postgres_custom">pg_dump custom (pg_restore)</SelectItem>
                </SelectContent>
              </Select>
            </div>
          )}

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

          {requiresConfirm && (
            <div>
              <Label className="text-xs text-muted-foreground">
                {t('restore.confirmLabel', { name: expectedDb })}
              </Label>
              <Input
                value={confirmDb}
                onChange={e => setConfirmDb(e.target.value)}
                disabled={phase !== 'idle'}
                placeholder={expectedDb}
                className={confirmOk ? '' : 'border-destructive/60'}
              />
            </div>
          )}

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
            <p className="text-sm text-green-500">{t('restore.completed')}</p>
          )}
        </div>

        <DialogFooter>
          {phase === 'idle' && (
            <>
              <Button variant="outline" onClick={onClose}>
                {t('common.cancel')}
              </Button>
              <Button
                variant="destructive"
                onClick={handleStart}
                disabled={!inputPath || !confirmOk}
              >
                <Upload size={14} className="mr-1" />
                {t('restore.start')}
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

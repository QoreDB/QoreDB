// SPDX-License-Identifier: Apache-2.0

import { open } from '@tauri-apps/plugin-dialog';
import { AlertTriangle, FileCode, Loader2, Play } from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
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
import { Switch } from '@/components/ui/switch';
import type { ImportSqlTarget } from '@/lib/stores/modalStore';
import { type ImportSqlResponse, importSql } from '@/lib/tauri/data-io';

interface ImportSqlDialogProps {
  target: ImportSqlTarget | null;
  open: boolean;
  onClose: () => void;
}

type Phase = 'idle' | 'running' | 'done';

export function ImportSqlDialog({ target, open: isOpen, onClose }: ImportSqlDialogProps) {
  const { t } = useTranslation();
  const [inputPath, setInputPath] = useState('');
  const [stopOnError, setStopOnError] = useState(true);
  const [phase, setPhase] = useState<Phase>('idle');
  const [result, setResult] = useState<ImportSqlResponse | null>(null);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    if (isOpen) {
      setInputPath('');
      setStopOnError(true);
      setPhase('idle');
      setResult(null);
      setError(null);
    }
  }, [isOpen]);

  const pickFile = useCallback(async () => {
    const chosen = await open({
      multiple: false,
      filters: [{ name: 'SQL', extensions: ['sql'] }],
    });
    if (typeof chosen === 'string') setInputPath(chosen);
  }, []);

  const handleRun = useCallback(async () => {
    if (!target || !inputPath) return;
    setPhase('running');
    setError(null);
    setResult(null);
    try {
      const res = await importSql(
        target.sessionId,
        target.database,
        target.schema,
        inputPath,
        stopOnError,
        true
      );
      setResult(res);
      if (res.error) setError(res.error);
      setPhase('done');
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
      setPhase('done');
    }
  }, [target, inputPath, stopOnError]);

  if (!target) return null;

  return (
    <Dialog open={isOpen} onOpenChange={o => !o && onClose()}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle>{t('importSql.dialogTitle', { name: target.label })}</DialogTitle>
        </DialogHeader>

        <div className="space-y-4 py-2">
          <div className="flex items-start gap-2 p-3 rounded-md bg-warning/10 border border-warning/40 text-xs">
            <AlertTriangle size={14} className="text-warning shrink-0 mt-0.5" />
            <span className="text-warning-foreground">
              {t('importSql.warning', { name: target.label })}
            </span>
          </div>

          <div>
            <Label className="text-xs text-muted-foreground">{t('importSql.inputPath')}</Label>
            <div className="flex items-center gap-2">
              <Input
                value={inputPath}
                readOnly
                placeholder={t('importSql.inputPathPlaceholder')}
                className="flex-1"
              />
              <Button variant="outline" onClick={pickFile} disabled={phase === 'running'}>
                {t('backup.browse')}
              </Button>
            </div>
          </div>

          <div className="flex items-center justify-between">
            <div>
              <Label className="text-xs">{t('importSql.stopOnError')}</Label>
              <p className="text-[11px] text-muted-foreground">{t('importSql.stopOnErrorHint')}</p>
            </div>
            <Switch
              checked={stopOnError}
              onCheckedChange={setStopOnError}
              disabled={phase === 'running'}
            />
          </div>

          {result && (
            <div className="space-y-2">
              <div className="flex gap-4 text-sm">
                <span className="text-muted-foreground">
                  {t('importSql.total', { count: result.total_statements })}
                </span>
                <span className="text-green-500">
                  {t('importSql.executed', { count: result.executed })}
                </span>
                {result.failed > 0 && (
                  <span className="text-destructive">
                    {t('importSql.failedCount', { count: result.failed })}
                  </span>
                )}
              </div>
              {result.errors.length > 0 && (
                <ScrollArea className="h-40 rounded border border-border bg-muted/30 p-2 font-mono text-xs">
                  {result.errors.map(err => (
                    <div key={err.statement_index} className="mb-2">
                      <div className="text-destructive">
                        #{err.statement_index + 1}: {err.message}
                      </div>
                      <div className="text-muted-foreground whitespace-pre-wrap break-all">
                        {err.statement_preview}
                      </div>
                    </div>
                  ))}
                </ScrollArea>
              )}
            </div>
          )}

          {error && <p className="text-sm text-destructive">{error}</p>}
          {phase === 'done' && result?.success && (
            <p className="text-sm text-green-500">{t('importSql.completed')}</p>
          )}
        </div>

        <DialogFooter>
          {phase !== 'done' && (
            <>
              <Button variant="outline" onClick={onClose} disabled={phase === 'running'}>
                {t('common.cancel')}
              </Button>
              <Button onClick={handleRun} disabled={!inputPath || phase === 'running'}>
                {phase === 'running' ? (
                  <Loader2 size={14} className="mr-1 animate-spin" />
                ) : (
                  <Play size={14} className="mr-1" />
                )}
                {phase === 'running' ? t('importSql.running') : t('importSql.run')}
              </Button>
            </>
          )}
          {phase === 'done' && (
            <>
              <Button
                variant="outline"
                onClick={() => {
                  setResult(null);
                  setError(null);
                  setPhase('idle');
                }}
              >
                <FileCode size={14} className="mr-1" />
                {t('importSql.runAnother')}
              </Button>
              <Button onClick={onClose}>{t('common.close')}</Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

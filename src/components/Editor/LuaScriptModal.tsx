// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, FileCode, Loader2, Play, Upload } from 'lucide-react';
import { useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Label } from '@/components/ui/label';
import { Textarea } from '@/components/ui/textarea';
import type { Environment } from '@/lib/tauri';
import { executeQuery } from '@/lib/tauri';

type ExecuteResponse = Awaited<ReturnType<typeof executeQuery>>;

import {
  buildEvalScript,
  buildEvalSha,
  buildScriptLoad,
  detectDangerousLuaCalls,
} from '@/lib/redisCommands';
import { LuaScriptEditor } from './LuaScriptEditor';

interface LuaScriptModalProps {
  isOpen: boolean;
  onClose: () => void;
  onSuccess?: () => void;
  sessionId: string;
  environment: Environment;
  connectionDatabase?: string;
}

const DEFAULT_SCRIPT = `-- Example: read a key and return its value
local value = redis.call('GET', KEYS[1])
return value`;

function parseLines(raw: string): string[] {
  return raw
    .split('\n')
    .map(line => line.trim())
    .filter(line => line.length > 0);
}

function formatResult(res: ExecuteResponse | null): string {
  if (!res) return '';
  if (!res.success) return res.error ?? '';
  try {
    return JSON.stringify(res.result ?? {}, null, 2);
  } catch {
    return String(res);
  }
}

export function LuaScriptModal({
  isOpen,
  onClose,
  onSuccess,
  sessionId,
  environment,
  connectionDatabase,
}: LuaScriptModalProps) {
  const { t } = useTranslation();
  const [script, setScript] = useState(DEFAULT_SCRIPT);
  const [keysRaw, setKeysRaw] = useState('');
  const [argsRaw, setArgsRaw] = useState('');
  const [lastSha, setLastSha] = useState<string | null>(null);
  const [result, setResult] = useState<ExecuteResponse | null>(null);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [confirmOpen, setConfirmOpen] = useState(false);
  const [pendingCommand, setPendingCommand] = useState<string | null>(null);

  const dangerousTokens = useMemo(() => detectDangerousLuaCalls(script), [script]);

  const keys = useMemo(() => parseLines(keysRaw), [keysRaw]);
  const args = useMemo(() => parseLines(argsRaw), [argsRaw]);

  function closeSelf() {
    if (loading) return;
    setScript(DEFAULT_SCRIPT);
    setKeysRaw('');
    setArgsRaw('');
    setLastSha(null);
    setResult(null);
    setError(null);
    setPendingCommand(null);
    setConfirmOpen(false);
    onClose();
  }

  async function runCommand(command: string, acknowledgedDangerous: boolean) {
    setLoading(true);
    setError(null);
    try {
      const res = await executeQuery(sessionId, command, { acknowledgedDangerous });
      setResult(res);
      if (res.success) {
        if (command.startsWith('SCRIPT LOAD')) {
          const sha = extractShaFromResult(res);
          if (sha) {
            setLastSha(sha);
            toast.success(t('redisLua.scriptLoaded', { sha: sha.slice(0, 12) }));
          } else {
            toast.success(t('redisLua.scriptLoaded', { sha: '…' }));
          }
        } else {
          toast.success(t('redisLua.executed'));
          onSuccess?.();
        }
      } else {
        setError(res.error ?? t('common.unknownError'));
      }
    } catch (err) {
      setError(err instanceof Error ? err.message : t('common.unknownError'));
    } finally {
      setLoading(false);
      setConfirmOpen(false);
      setPendingCommand(null);
    }
  }

  function submit(commandBuilder: () => string) {
    setError(null);
    try {
      const command = commandBuilder();
      if (environment !== 'development') {
        setPendingCommand(command);
        setConfirmOpen(true);
        return;
      }
      void runCommand(command, false);
    } catch (err) {
      setError(err instanceof Error ? err.message : t('common.unknownError'));
    }
  }

  function handleEval() {
    submit(() => buildEvalScript({ script, keys, args }));
  }

  function handleEvalSha() {
    if (!lastSha) return;
    submit(() => buildEvalSha({ sha: lastSha, keys, args }));
  }

  function handleScriptLoad() {
    submit(() => buildScriptLoad(script));
  }

  return (
    <>
      <Dialog open={isOpen} onOpenChange={open => !open && closeSelf()}>
        <DialogContent className="max-w-3xl">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <FileCode size={18} />
              {t('redisLua.title')}
              {connectionDatabase && (
                <span className="text-xs text-muted-foreground font-mono ml-2">
                  {connectionDatabase}
                </span>
              )}
            </DialogTitle>
          </DialogHeader>

          <div className="space-y-4 max-h-[70vh] overflow-y-auto pr-1">
            <div className="space-y-2">
              <Label>{t('redisLua.scriptLabel')}</Label>
              <LuaScriptEditor value={script} onChange={setScript} onExecute={handleEval} />
              <p className="text-xs text-muted-foreground">{t('redisLua.scriptHint')}</p>
            </div>

            {dangerousTokens.length > 0 && (
              <div className="rounded-md bg-warning/10 border border-warning/30 text-warning-foreground px-3 py-2 flex items-start gap-2 text-sm">
                <AlertTriangle size={16} className="mt-0.5 shrink-0" />
                <div>
                  <div className="font-medium">{t('redisLua.dangerousWarning')}</div>
                  <div className="text-xs text-muted-foreground mt-0.5 font-mono">
                    {dangerousTokens.join(', ')}
                  </div>
                </div>
              </div>
            )}

            <div className="grid grid-cols-1 md:grid-cols-2 gap-4">
              <div className="space-y-2">
                <Label htmlFor="redis-lua-keys">{t('redisLua.keysLabel')}</Label>
                <Textarea
                  id="redis-lua-keys"
                  value={keysRaw}
                  onChange={e => setKeysRaw(e.target.value)}
                  placeholder={t('redisLua.keysPlaceholder')}
                  rows={4}
                  className="font-mono text-xs"
                />
                <p className="text-xs text-muted-foreground">
                  {t('redisLua.keysCount', { count: keys.length })}
                </p>
              </div>
              <div className="space-y-2">
                <Label htmlFor="redis-lua-args">{t('redisLua.argsLabel')}</Label>
                <Textarea
                  id="redis-lua-args"
                  value={argsRaw}
                  onChange={e => setArgsRaw(e.target.value)}
                  placeholder={t('redisLua.argsPlaceholder')}
                  rows={4}
                  className="font-mono text-xs"
                />
                <p className="text-xs text-muted-foreground">
                  {t('redisLua.argsCount', { count: args.length })}
                </p>
              </div>
            </div>

            {lastSha && (
              <div className="rounded-md bg-muted/50 border border-border px-3 py-2 text-xs font-mono flex items-center justify-between gap-2">
                <span className="text-muted-foreground">{t('redisLua.lastSha')}</span>
                <span className="truncate">{lastSha}</span>
              </div>
            )}

            {result && (
              <div className="space-y-2">
                <Label>{t('redisLua.resultLabel')}</Label>
                <pre className="rounded-md border border-border bg-muted/50 p-3 text-xs font-mono overflow-x-auto max-h-48">
                  {formatResult(result)}
                </pre>
              </div>
            )}

            {error && (
              <div className="rounded-md bg-error/10 border border-error/20 text-error text-sm px-3 py-2">
                {error}
              </div>
            )}
          </div>

          <DialogFooter className="gap-2 flex-wrap sm:flex-nowrap">
            <Button variant="ghost" onClick={closeSelf} disabled={loading}>
              {t('common.close')}
            </Button>
            <Button
              variant="outline"
              onClick={handleScriptLoad}
              disabled={loading}
              className="gap-2"
            >
              <Upload size={14} />
              {t('redisLua.scriptLoad')}
            </Button>
            {lastSha && (
              <Button
                variant="outline"
                onClick={handleEvalSha}
                disabled={loading}
                className="gap-2"
              >
                <Play size={14} />
                {t('redisLua.runViaSha')}
              </Button>
            )}
            <Button onClick={handleEval} disabled={loading} className="gap-2">
              {loading && <Loader2 size={14} className="animate-spin" />}
              {!loading && <Play size={14} />}
              {t('redisLua.run')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DangerConfirmDialog
        open={confirmOpen}
        onOpenChange={setConfirmOpen}
        title={t('redisLua.confirmTitle')}
        description={t('redisLua.confirmDesc')}
        confirmLabel={t('redisLua.run')}
        loading={loading}
        onConfirm={() => {
          if (pendingCommand) {
            void runCommand(pendingCommand, true);
          }
        }}
      />
    </>
  );
}

function extractShaFromResult(res: ExecuteResponse): string | null {
  if (!res.success || !res.result) return null;
  const rows = res.result.rows;
  if (!Array.isArray(rows) || rows.length === 0) return null;
  for (const row of rows) {
    for (const cell of row.values ?? []) {
      if (typeof cell === 'string' && /^[a-f0-9]{40}$/i.test(cell)) {
        return cell.toLowerCase();
      }
    }
  }
  return null;
}

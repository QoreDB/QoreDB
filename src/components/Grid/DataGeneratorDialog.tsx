// SPDX-License-Identifier: BUSL-1.1

import { save } from '@tauri-apps/plugin-dialog';
import { writeTextFile } from '@tauri-apps/plugin-fs';
import { ClipboardCopy, Download, Loader2, Play, Sparkles } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { SqlPreview } from '@/components/Sandbox/SqlPreview';
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
import { Driver } from '@/lib/connection/drivers';
import { generateSeedData, type SeedDataResult } from '@/lib/dataGenerator';
import { executeQuery, type Namespace } from '@/lib/tauri';

interface DataGeneratorDialogProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  sessionId: string;
  namespace: Namespace;
  tableName: string;
  connectionId?: string;
  driver?: Driver;
  /** Called after rows are inserted so the grid can refresh. */
  onExecuted?: () => void;
}

const DEFAULT_COUNT = 50;
const MAX_COUNT = 10_000;

export function DataGeneratorDialog({
  open,
  onOpenChange,
  sessionId,
  namespace,
  tableName,
  connectionId,
  driver = Driver.Postgres,
  onExecuted,
}: DataGeneratorDialogProps) {
  const { t } = useTranslation();
  const [count, setCount] = useState(DEFAULT_COUNT);
  const [loading, setLoading] = useState(false);
  const [executing, setExecuting] = useState(false);
  const [result, setResult] = useState<SeedDataResult | null>(null);
  const [error, setError] = useState<string | null>(null);

  async function handleGenerate() {
    setLoading(true);
    setError(null);
    setResult(null);
    try {
      const res = await generateSeedData(sessionId, namespace, tableName, count, connectionId);
      setResult(res);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : String(err));
    } finally {
      setLoading(false);
    }
  }

  async function handleExecute() {
    if (!result) return;
    const statements = result.sql
      .split('\n\n')
      .map(s => s.trim())
      .filter(Boolean);
    setExecuting(true);
    try {
      for (const stmt of statements) {
        const resp = await executeQuery(sessionId, stmt, { namespace });
        if (!resp.success) {
          throw new Error(resp.error ?? t('query.unknownError'));
        }
      }
      toast.success(t('dataGenerator.executed', { count: result.rowCount }));
      onExecuted?.();
      onOpenChange(false);
    } catch (err) {
      setError(typeof err === 'string' ? err : err instanceof Error ? err.message : String(err));
    } finally {
      setExecuting(false);
    }
  }

  async function handleExport() {
    if (!result) return;
    try {
      const filePath = await save({
        defaultPath: `${tableName}-seed.sql`,
        filters: [{ name: 'SQL', extensions: ['sql'] }],
      });
      if (!filePath) return;
      await writeTextFile(filePath, result.sql);
      toast.success(t('dataGenerator.exported'));
    } catch (err) {
      toast.error(t('dataGenerator.exportError'), {
        description: err instanceof Error ? err.message : String(err),
      });
    }
  }

  function handleCopy() {
    if (!result) return;
    navigator.clipboard.writeText(result.sql);
    toast.success(t('dataGenerator.copied'));
  }

  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="max-w-2xl">
        <DialogHeader>
          <DialogTitle className="flex items-center gap-2">
            <Sparkles size={16} className="text-accent" />
            {t('dataGenerator.title', { table: tableName })}
          </DialogTitle>
        </DialogHeader>

        <div className="grid gap-4 py-2">
          <div className="flex items-end gap-3">
            <div className="grid gap-2">
              <Label htmlFor="dg-count">{t('dataGenerator.rowCount')}</Label>
              <Input
                id="dg-count"
                type="number"
                min={1}
                max={MAX_COUNT}
                value={count}
                onChange={e =>
                  setCount(Math.max(1, Math.min(MAX_COUNT, Number(e.target.value) || 1)))
                }
                className="w-32"
              />
            </div>
            <Button onClick={handleGenerate} disabled={loading} className="gap-1.5">
              {loading ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
              {t('dataGenerator.generate')}
            </Button>
          </div>

          {error && (
            <div className="rounded-md bg-destructive/10 px-3 py-2 text-sm text-destructive">
              {error}
            </div>
          )}

          {result && result.warnings.length > 0 && (
            <ul className="rounded-md border border-warning/20 bg-warning/10 px-3 py-2 text-xs text-warning">
              {result.warnings.map(w => (
                <li key={w}>{w}</li>
              ))}
            </ul>
          )}

          {result && (
            <div className="h-64 overflow-hidden rounded-md border border-border">
              <SqlPreview value={result.sql} dialect={driver} className="h-full" />
            </div>
          )}
        </div>

        <DialogFooter className="gap-2">
          {result && (
            <>
              <Button variant="ghost" onClick={handleCopy} className="gap-1.5">
                <ClipboardCopy size={14} />
                {t('dataGenerator.copy')}
              </Button>
              <Button variant="outline" onClick={handleExport} className="gap-1.5">
                <Download size={14} />
                {t('dataGenerator.export')}
              </Button>
              <Button onClick={handleExecute} disabled={executing} className="gap-1.5">
                {executing ? <Loader2 size={14} className="animate-spin" /> : <Play size={14} />}
                {t('dataGenerator.execute')}
              </Button>
            </>
          )}
        </DialogFooter>
      </DialogContent>
    </Dialog>
  );
}

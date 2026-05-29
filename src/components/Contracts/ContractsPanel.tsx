// SPDX-License-Identifier: BUSL-1.1

import { FileText, Pencil, Play, Plus, RefreshCw, Trash2 } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';

import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  type ContractMeta,
  type ContractRun,
  deleteContract,
  listContracts,
  loadContract,
} from '@/lib/contracts';

import { ContractEditor } from './ContractEditor';
import { ContractHealthBadge, deriveHealth } from './ContractHealthBadge';
import { ContractRunDialog } from './ContractRunDialog';

interface Props {
  open: boolean;
  onClose: () => void;
  /** Active session id (optional — needed to run contracts). */
  sessionId: string | null;
  /** Active saved connection id (optional). */
  connectionId?: string | null;
}

type Mode = { kind: 'list' } | { kind: 'new' } | { kind: 'edit'; name: string; source: string };

export function ContractsPanel({ open, onClose, sessionId, connectionId }: Props) {
  const { t } = useTranslation();
  const [contracts, setContracts] = useState<ContractMeta[]>([]);
  const [loading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [mode, setMode] = useState<Mode>({ kind: 'list' });
  const [runTarget, setRunTarget] = useState<{ name: string; source: string } | null>(null);
  const [dirty, setDirty] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const list = await listContracts();
      setContracts(list);
    } catch (e) {
      const msg = e instanceof Error ? e.message : String(e);
      setError(msg);
    } finally {
      setLoading(false);
    }
  }, []);

  // Initial load + every time the dialog re-opens.
  useEffect(() => {
    if (open) {
      refresh();
      setMode({ kind: 'list' });
    }
  }, [open, refresh]);

  const summary = useMemo(() => summarize(contracts), [contracts]);

  async function handleEdit(meta: ContractMeta) {
    try {
      const source = await loadContract(meta.name);
      setMode({ kind: 'edit', name: meta.name, source });
    } catch (e) {
      toast.error(t('contracts.errors.loadFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function handleRun(meta: ContractMeta) {
    try {
      const source = await loadContract(meta.name);
      setRunTarget({ name: meta.name, source });
    } catch (e) {
      toast.error(t('contracts.errors.runFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function handleDelete(meta: ContractMeta) {
    const ok = window.confirm(
      `${t('contracts.confirm.deleteTitle', { name: meta.name })}\n\n${t(
        'contracts.confirm.deleteBody'
      )}`
    );
    if (!ok) return;

    try {
      await deleteContract(meta.name);
      await refresh();
    } catch (e) {
      toast.error(t('contracts.errors.deleteFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  function handleClose() {
    if ((mode.kind === 'new' || mode.kind === 'edit') && dirty) {
      const ok = window.confirm(t('contracts.unsavedChanges'));
      if (!ok) return;
    }
    onClose();
  }

  return (
    <>
      <Dialog open={open} onOpenChange={v => !v && handleClose()}>
        <DialogContent className="max-w-5xl max-h-[85vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>{t('contracts.title')}</DialogTitle>
            <DialogDescription>{t('contracts.description')}</DialogDescription>
          </DialogHeader>

          {mode.kind === 'list' && (
            <div className="flex flex-col min-h-0 flex-1 gap-3">
              <HeaderToolbar
                summary={summary}
                loading={loading}
                onRefresh={refresh}
                onNew={() => setMode({ kind: 'new' })}
              />
              {error && <ErrorBanner message={error} />}
              <ContractList
                contracts={contracts}
                loading={loading}
                onEdit={handleEdit}
                onRun={handleRun}
                onDelete={handleDelete}
                onCreate={() => setMode({ kind: 'new' })}
              />
            </div>
          )}

          {mode.kind === 'new' && (
            <ContractEditor
              initialSource=""
              initialName=""
              nameLocked={false}
              onCancel={() => setMode({ kind: 'list' })}
              onSaved={() => {
                setMode({ kind: 'list' });
                refresh();
              }}
              onDirtyChange={setDirty}
            />
          )}

          {mode.kind === 'edit' && (
            <ContractEditor
              initialSource={mode.source}
              initialName={mode.name}
              nameLocked
              onCancel={() => setMode({ kind: 'list' })}
              onSaved={() => {
                setMode({ kind: 'list' });
                refresh();
              }}
              onDirtyChange={setDirty}
            />
          )}

          {mode.kind === 'list' && (
            <DialogFooter>
              <Button variant="ghost" onClick={handleClose}>
                {t('contracts.close')}
              </Button>
            </DialogFooter>
          )}
        </DialogContent>
      </Dialog>

      <ContractRunDialog
        open={runTarget !== null}
        onClose={() => {
          setRunTarget(null);
          refresh();
        }}
        sessionId={sessionId}
        connectionId={connectionId}
        contractName={runTarget?.name ?? ''}
        contractSource={runTarget?.source ?? ''}
      />
    </>
  );
}

interface Summary {
  total: number;
  healthy: number;
  failing: number;
  warning: number;
  unknown: number;
}

function summarize(contracts: ContractMeta[]): Summary {
  const summary: Summary = {
    total: contracts.length,
    healthy: 0,
    failing: 0,
    warning: 0,
    unknown: 0,
  };
  for (const c of contracts) {
    const lvl = deriveHealth(c.last_run);
    summary[lvl] += 1;
  }
  return summary;
}

interface HeaderToolbarProps {
  summary: Summary;
  loading: boolean;
  onRefresh: () => void;
  onNew: () => void;
}

function HeaderToolbar({ summary, loading, onRefresh, onNew }: HeaderToolbarProps) {
  const { t } = useTranslation();
  return (
    <div className="flex items-center justify-between gap-4">
      <div className="flex items-center gap-3 text-sm text-muted-foreground">
        <span>
          <strong className="text-foreground tabular-nums">{summary.total}</strong>{' '}
          {t('contracts.title')}
        </span>
        {summary.failing > 0 && (
          <ContractHealthBadge level="failing" withLabel={false} className="!px-1.5" />
        )}
        {summary.warning > 0 && (
          <ContractHealthBadge level="warning" withLabel={false} className="!px-1.5" />
        )}
        {summary.healthy > 0 && (
          <ContractHealthBadge level="healthy" withLabel={false} className="!px-1.5" />
        )}
      </div>
      <div className="flex items-center gap-2">
        <Button variant="ghost" size="sm" onClick={onRefresh} disabled={loading}>
          <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
        </Button>
        <Button size="sm" onClick={onNew}>
          <Plus size={14} />
          {t('contracts.newContract')}
        </Button>
      </div>
    </div>
  );
}

function ErrorBanner({ message }: { message: string }) {
  return (
    <div className="text-xs text-red-600 dark:text-red-400 px-3 py-2 rounded border border-red-500/30 bg-red-500/10">
      {message}
    </div>
  );
}

interface ContractListProps {
  contracts: ContractMeta[];
  loading: boolean;
  onEdit: (meta: ContractMeta) => void;
  onRun: (meta: ContractMeta) => void;
  onDelete: (meta: ContractMeta) => void;
  onCreate: () => void;
}

function ContractList({
  contracts,
  loading,
  onEdit,
  onRun,
  onDelete,
  onCreate,
}: ContractListProps) {
  const { t } = useTranslation();

  if (!loading && contracts.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center px-6 py-10 border border-dashed border-border rounded-md">
        <FileText size={28} className="text-muted-foreground/50" />
        <h3 className="text-sm font-medium">{t('contracts.empty.title')}</h3>
        <p className="text-xs text-muted-foreground max-w-md">{t('contracts.empty.description')}</p>
        <Button size="sm" onClick={onCreate} className="mt-2">
          <Plus size={14} />
          {t('contracts.empty.createFirst')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto rounded-md border border-border">
      <table className="w-full text-sm">
        <thead className="bg-muted/40 text-xs uppercase tracking-wider text-muted-foreground sticky top-0">
          <tr>
            <th className="px-3 py-2 text-left font-medium">{t('contracts.list.name')}</th>
            <th className="px-3 py-2 text-left font-medium">{t('contracts.list.status')}</th>
            <th className="px-3 py-2 text-right font-medium">{t('contracts.list.lastRun')}</th>
            <th className="px-3 py-2 text-right font-medium w-32" />
          </tr>
        </thead>
        <tbody>
          {contracts.map(meta => (
            <ContractRow
              key={meta.id}
              meta={meta}
              onEdit={onEdit}
              onRun={onRun}
              onDelete={onDelete}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface RowProps {
  meta: ContractMeta;
  onEdit: (meta: ContractMeta) => void;
  onRun: (meta: ContractMeta) => void;
  onDelete: (meta: ContractMeta) => void;
}

function ContractRow({ meta, onEdit, onRun, onDelete }: RowProps) {
  const { t } = useTranslation();
  return (
    <tr className="border-t border-border hover:bg-muted/30">
      <td className="px-3 py-2 align-middle">
        <div className="flex flex-col">
          <span className="font-medium text-foreground">{meta.name}</span>
          <span className="text-xs text-muted-foreground">
            {t('contracts.list.rulesCount', { count: meta.rules_count })}
          </span>
        </div>
      </td>
      <td className="px-3 py-2 align-middle">
        <ContractHealthBadge run={meta.last_run} />
      </td>
      <td className="px-3 py-2 align-middle text-right text-xs text-muted-foreground tabular-nums">
        {formatLastRun(meta.last_run, t)}
      </td>
      <td className="px-3 py-2 align-middle text-right">
        <div className="inline-flex items-center gap-1">
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onEdit(meta)}
            aria-label={t('contracts.edit')}
            title={t('contracts.edit')}
          >
            <Pencil size={13} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onRun(meta)}
            aria-label={t('contracts.run.action')}
            title={t('contracts.run.action')}
          >
            <Play size={13} />
          </Button>
          <Button
            variant="ghost"
            size="sm"
            onClick={() => onDelete(meta)}
            aria-label={t('contracts.delete')}
            title={t('contracts.delete')}
            className="text-muted-foreground hover:text-red-600 dark:hover:text-red-400"
          >
            <Trash2 size={13} />
          </Button>
        </div>
      </td>
    </tr>
  );
}

function formatLastRun(run: ContractRun | null | undefined, t: (k: string) => string): string {
  if (!run) return t('contracts.list.neverRun');
  try {
    const date = new Date(run.finished_at);
    return date.toLocaleString();
  } catch {
    return run.finished_at;
  }
}

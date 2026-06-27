// SPDX-License-Identifier: BUSL-1.1

import {
  Activity,
  FileJson,
  Globe,
  KeyRound,
  Play,
  Plus,
  Power,
  RefreshCw,
  Trash2,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
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
  type CreateEndpointResponse,
  deleteEndpoint,
  type EndpointMeta,
  getInstantApiStatus,
  type InstantApiStatus,
  listEndpoints,
  regenerateEndpointToken,
  startInstantApi,
  stopInstantApi,
} from '@/lib/instantApi';
import { confirmDialog } from '@/lib/stores/confirmStore';

import { EndpointDialog } from './EndpointDialog';
import { EndpointTokenDialog } from './EndpointTokenDialog';
import { OpenApiPreview } from './OpenApiPreview';

interface Props {
  open: boolean;
  onClose: () => void;
}

export function InstantApiPanel({ open, onClose }: Props) {
  const { t } = useTranslation();
  const [status, setStatus] = useState<InstantApiStatus | null>(null);
  const [endpoints, setEndpoints] = useState<EndpointMeta[]>([]);
  const [loading, setLoading] = useState(false);
  const [busy, setBusy] = useState(false);
  const [newOpen, setNewOpen] = useState(false);
  const [openApiOpen, setOpenApiOpen] = useState(false);
  const [tokenView, setTokenView] = useState<CreateEndpointResponse | null>(null);
  const [tlsPref, setTlsPref] = useState(false);

  const refresh = useCallback(async () => {
    setLoading(true);
    try {
      const [st, list] = await Promise.all([getInstantApiStatus(), listEndpoints()]);
      setStatus(st);
      setEndpoints(list);
    } catch (e) {
      toast.error(t('instantApi.errors.statusFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    } finally {
      setLoading(false);
    }
  }, [t]);

  useEffect(() => {
    if (open) {
      void refresh();
    }
  }, [open, refresh]);

  async function toggleServer() {
    setBusy(true);
    try {
      const next = status?.running
        ? await stopInstantApi()
        : await startInstantApi(undefined, { tls: tlsPref });
      setStatus(next);
    } catch (e) {
      toast.error(
        status?.running ? t('instantApi.errors.stopFailed') : t('instantApi.errors.startFailed'),
        { description: e instanceof Error ? e.message : String(e) }
      );
    } finally {
      setBusy(false);
    }
  }

  async function handleDelete(meta: EndpointMeta) {
    const ok = await confirmDialog({
      description: t('instantApi.confirmDelete', { name: meta.name }),
    });
    if (!ok) return;
    try {
      await deleteEndpoint(meta.id);
      await refresh();
    } catch (e) {
      toast.error(t('instantApi.errors.deleteFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  async function handleRegenerate(meta: EndpointMeta) {
    const ok = await confirmDialog({
      description: t('instantApi.regenerateConfirm', { name: meta.name }),
    });
    if (!ok) return;
    try {
      const response = await regenerateEndpointToken(meta.id);
      setTokenView(response);
    } catch (e) {
      toast.error(t('instantApi.regenerateFailed'), {
        description: e instanceof Error ? e.message : String(e),
      });
    }
  }

  function handleCreated(response: CreateEndpointResponse) {
    setTokenView(response);
    void refresh();
  }

  const tokenUrl =
    tokenView && status?.base_url
      ? `${status.base_url}/api/${tokenView.endpoint.name}`
      : tokenView
        ? `http://127.0.0.1:4787/api/${tokenView.endpoint.name}`
        : '';

  return (
    <>
      <Dialog open={open} onOpenChange={v => !v && onClose()}>
        <DialogContent className="max-w-4xl max-h-[85vh] flex flex-col">
          <DialogHeader>
            <DialogTitle>{t('instantApi.title')}</DialogTitle>
            <DialogDescription>{t('instantApi.description')}</DialogDescription>
          </DialogHeader>

          <div className="flex flex-col gap-3 min-h-0 flex-1">
            <ServerStatusBar
              status={status}
              loading={loading}
              busy={busy}
              onToggle={toggleServer}
              onRefresh={refresh}
              tlsPref={tlsPref}
              onToggleTls={() => setTlsPref(v => !v)}
            />

            <div className="flex items-center justify-between">
              <div className="text-sm text-muted-foreground">
                <strong className="text-foreground tabular-nums">{endpoints.length}</strong>{' '}
                {t('instantApi.endpointsLabel')}
              </div>
              <div className="flex items-center gap-2">
                <Button
                  variant="outline"
                  size="sm"
                  onClick={() => setOpenApiOpen(true)}
                  disabled={endpoints.length === 0}
                >
                  <FileJson size={13} />
                  {t('instantApi.openapi.open')}
                </Button>
                <Button size="sm" onClick={() => setNewOpen(true)}>
                  <Plus size={14} />
                  {t('instantApi.newEndpoint')}
                </Button>
              </div>
            </div>

            <EndpointList
              endpoints={endpoints}
              baseUrl={status?.base_url ?? null}
              loading={loading}
              onCreate={() => setNewOpen(true)}
              onDelete={handleDelete}
              onRegenerate={handleRegenerate}
            />
          </div>

          <DialogFooter>
            <Button variant="ghost" onClick={onClose}>
              {t('instantApi.close')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <EndpointDialog open={newOpen} onClose={() => setNewOpen(false)} onCreated={handleCreated} />

      <OpenApiPreview open={openApiOpen} onClose={() => setOpenApiOpen(false)} />

      <EndpointTokenDialog
        open={tokenView !== null}
        onClose={() => setTokenView(null)}
        token={tokenView?.token ?? ''}
        url={tokenUrl}
      />
    </>
  );
}

interface StatusBarProps {
  status: InstantApiStatus | null;
  loading: boolean;
  busy: boolean;
  onToggle: () => void;
  onRefresh: () => void;
  tlsPref: boolean;
  onToggleTls: () => void;
}

function ServerStatusBar({
  status,
  loading,
  busy,
  onToggle,
  onRefresh,
  tlsPref,
  onToggleTls,
}: StatusBarProps) {
  const { t } = useTranslation();
  const running = status?.running ?? false;
  const tlsActive = running && (status?.tls ?? false);

  return (
    <div className="flex items-center justify-between gap-3 px-3 py-2.5 rounded-md border border-border bg-muted/30">
      <div className="flex items-center gap-2.5 min-w-0">
        <div className="relative inline-flex">
          <Activity
            size={16}
            className={running ? 'text-emerald-500' : 'text-muted-foreground/60'}
          />
          {running && (
            <span className="absolute inset-0 inline-flex h-full w-full rounded-full bg-emerald-500/40 animate-ping" />
          )}
        </div>
        <div className="flex flex-col min-w-0">
          <span className="text-sm font-medium inline-flex items-center gap-1.5">
            {running ? t('instantApi.status.running') : t('instantApi.status.stopped')}
            {tlsActive && (
              <span
                className="inline-flex items-center px-1.5 py-0.5 rounded text-[9px] uppercase tracking-wide font-semibold bg-emerald-500/15 text-emerald-700 dark:text-emerald-400"
                title={t('instantApi.tls.selfSigned')}
              >
                {t('instantApi.tls.badge')}
              </span>
            )}
          </span>
          {status?.base_url && (
            <span className="text-[11px] text-muted-foreground font-mono truncate">
              {status.base_url}
            </span>
          )}
        </div>
      </div>
      <div className="flex items-center gap-2">
        {!running && (
          <label className="inline-flex items-center gap-1.5 text-[11px] text-muted-foreground select-none cursor-pointer">
            <input
              type="checkbox"
              checked={tlsPref}
              onChange={onToggleTls}
              className="h-3 w-3 accent-primary"
              disabled={busy}
            />
            {t('instantApi.tls.useHttps')}
          </label>
        )}
        <Button variant="ghost" size="sm" onClick={onRefresh} disabled={loading}>
          <RefreshCw size={13} className={loading ? 'animate-spin' : ''} />
        </Button>
        <Button
          variant={running ? 'destructive' : 'default'}
          size="sm"
          onClick={onToggle}
          disabled={busy}
        >
          <Power size={13} />
          {running ? t('instantApi.status.stop') : t('instantApi.status.start')}
        </Button>
      </div>
    </div>
  );
}

interface EndpointListProps {
  endpoints: EndpointMeta[];
  baseUrl: string | null;
  loading: boolean;
  onCreate: () => void;
  onDelete: (meta: EndpointMeta) => void;
  onRegenerate: (meta: EndpointMeta) => void;
}

function EndpointList({
  endpoints,
  baseUrl,
  loading,
  onCreate,
  onDelete,
  onRegenerate,
}: EndpointListProps) {
  const { t } = useTranslation();

  if (!loading && endpoints.length === 0) {
    return (
      <div className="flex-1 flex flex-col items-center justify-center gap-3 text-center px-6 py-10 border border-dashed border-border rounded-md">
        <Globe size={28} className="text-muted-foreground/50" />
        <h3 className="text-sm font-medium">{t('instantApi.empty.title')}</h3>
        <p className="text-xs text-muted-foreground max-w-md">
          {t('instantApi.empty.description')}
        </p>
        <Button size="sm" onClick={onCreate} className="mt-2">
          <Plus size={14} />
          {t('instantApi.empty.createFirst')}
        </Button>
      </div>
    );
  }

  return (
    <div className="flex-1 overflow-auto rounded-md border border-border">
      <table className="w-full text-sm">
        <thead className="bg-muted/40 text-xs uppercase tracking-wider text-muted-foreground sticky top-0">
          <tr>
            <th className="px-3 py-2 text-left font-medium">{t('instantApi.list.name')}</th>
            <th className="px-3 py-2 text-left font-medium">{t('instantApi.list.shape')}</th>
            <th className="px-3 py-2 text-left font-medium">{t('instantApi.list.url')}</th>
            <th className="px-3 py-2 text-right font-medium w-24" />
          </tr>
        </thead>
        <tbody>
          {endpoints.map(meta => (
            <EndpointRow
              key={meta.id}
              meta={meta}
              baseUrl={baseUrl}
              onDelete={() => onDelete(meta)}
              onRegenerate={() => onRegenerate(meta)}
            />
          ))}
        </tbody>
      </table>
    </div>
  );
}

interface RowProps {
  meta: EndpointMeta;
  baseUrl: string | null;
  onDelete: () => void;
  onRegenerate: () => void;
}

function EndpointRow({ meta, baseUrl, onDelete, onRegenerate }: RowProps) {
  const { t } = useTranslation();
  const url = baseUrl ? `${baseUrl}/api/${meta.name}` : `/api/${meta.name}`;

  function handleOpen() {
    if (!baseUrl) {
      toast.warning(t('instantApi.list.serverStopped'));
      return;
    }
    window.open(url, '_blank', 'noopener,noreferrer');
  }

  return (
    <tr className="border-t border-border hover:bg-muted/30">
      <td className="px-3 py-2 align-middle">
        <div className="flex flex-col">
          <span className="font-medium text-foreground">{meta.name}</span>
          <span className="text-[11px] text-muted-foreground">
            {t('instantApi.list.paramsCount', { count: meta.params_count })} ·{' '}
            {t('instantApi.list.pageSize', { count: meta.page_size })}
          </span>
        </div>
      </td>
      <td className="px-3 py-2 align-middle">
        <span className="inline-flex px-2 py-0.5 text-[11px] font-mono rounded bg-muted text-muted-foreground">
          {meta.shape}
        </span>
      </td>
      <td className="px-3 py-2 align-middle">
        <button
          type="button"
          onClick={handleOpen}
          className="flex items-center gap-1.5 text-xs font-mono text-muted-foreground hover:text-foreground truncate max-w-[20rem]"
        >
          <Play size={11} />
          <span className="truncate">{url}</span>
        </button>
      </td>
      <td className="px-3 py-2 align-middle text-right whitespace-nowrap">
        <Button variant="ghost" size="sm" onClick={onRegenerate} title={t('instantApi.regenerate')}>
          <KeyRound size={13} />
        </Button>
        <Button variant="ghost" size="sm" onClick={onDelete}>
          <Trash2 size={13} />
        </Button>
      </td>
    </tr>
  );
}

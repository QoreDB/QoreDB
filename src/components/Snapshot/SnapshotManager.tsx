// SPDX-License-Identifier: Apache-2.0

import {
  AlertCircle,
  Calendar,
  Camera,
  Database,
  Eye,
  GitCompare,
  HardDrive,
  Link2,
  Loader2,
  Pencil,
  Search,
  Table2,
  Trash2,
  X,
} from 'lucide-react';
import { useCallback, useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import {
  ShareExportDialog,
  type ShareExportDialogRequest,
} from '@/components/Share/ShareExportDialog';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { Input } from '@/components/ui/input';
import { useShareLinks } from '@/hooks/useShareLinks';
import { notify } from '@/lib/notify';
import type { QueryResult, SnapshotMeta } from '@/lib/tauri';
import { deleteSnapshot, getSnapshot, listSnapshots, renameSnapshot } from '@/lib/tauri';
import { cn } from '@/lib/utils';
import { DataGrid } from '../Grid/DataGrid';

interface SnapshotManagerProps {
  onCompareInDiff?: (snapshotId: string, snapshotMeta: SnapshotMeta) => void;
}

function formatFileSize(bytes: number): string {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`;
}

function formatRelativeTime(isoDate: string): string {
  const date = new Date(isoDate);
  const now = new Date();
  const diffMs = now.getTime() - date.getTime();
  const diffMin = Math.floor(diffMs / 60000);
  const diffHour = Math.floor(diffMin / 60);
  const diffDay = Math.floor(diffHour / 24);

  if (diffMin < 1) return 'just now';
  if (diffMin < 60) return `${diffMin}m ago`;
  if (diffHour < 24) return `${diffHour}h ago`;
  if (diffDay < 30) return `${diffDay}d ago`;
  return date.toLocaleDateString();
}

export function SnapshotManager({ onCompareInDiff }: SnapshotManagerProps) {
  const { t } = useTranslation();
  const [snapshots, setSnapshots] = useState<SnapshotMeta[]>([]);
  const [loading, setLoading] = useState(true);
  const [search, setSearch] = useState('');

  // Preview state
  const [previewSnapshot, setPreviewSnapshot] = useState<{
    meta: SnapshotMeta;
    result: QueryResult;
  } | null>(null);
  const [previewLoading, setPreviewLoading] = useState(false);

  // Rename state
  const [renameTarget, setRenameTarget] = useState<SnapshotMeta | null>(null);
  const [renameName, setRenameName] = useState('');

  // Delete state
  const [deleteTarget, setDeleteTarget] = useState<SnapshotMeta | null>(null);
  const [shareTarget, setShareTarget] = useState<SnapshotMeta | null>(null);
  const { shareSnapshot } = useShareLinks();

  const loadSnapshots = useCallback(async () => {
    setLoading(true);
    try {
      const response = await listSnapshots();
      if (response.success) {
        setSnapshots(response.snapshots);
      }
    } catch {
      console.error('Failed to load snapshots');
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    loadSnapshots();
  }, [loadSnapshots]);

  const handlePreview = async (meta: SnapshotMeta) => {
    setPreviewLoading(true);
    try {
      const response = await getSnapshot(meta.id);
      if (response.success && response.result && response.meta) {
        setPreviewSnapshot({ meta: response.meta, result: response.result });
      } else {
        notify.error(response.error ?? t('snapshots.loadError'));
      }
    } catch {
      notify.error(t('snapshots.loadError'));
    } finally {
      setPreviewLoading(false);
    }
  };

  const handleDelete = async () => {
    if (!deleteTarget) return;
    try {
      const response = await deleteSnapshot(deleteTarget.id);
      if (response.success) {
        notify.success(t('snapshots.deleteSuccess'));
        setSnapshots(prev => prev.filter(s => s.id !== deleteTarget.id));
        if (previewSnapshot?.meta.id === deleteTarget.id) {
          setPreviewSnapshot(null);
        }
      } else {
        notify.error(response.error ?? t('snapshots.deleteError'));
      }
    } catch {
      notify.error(t('snapshots.deleteError'));
    } finally {
      setDeleteTarget(null);
    }
  };

  const handleRename = async () => {
    if (!renameTarget || !renameName.trim()) return;
    try {
      const response = await renameSnapshot(renameTarget.id, renameName.trim());
      const { meta } = response;
      if (response.success && meta) {
        notify.success(t('snapshots.renameSuccess'));
        setSnapshots(prev => prev.map(s => (s.id === renameTarget.id ? meta : s)));
        if (previewSnapshot?.meta.id === renameTarget.id) {
          setPreviewSnapshot({ ...previewSnapshot, meta });
        }
      } else {
        notify.error(response.error ?? t('snapshots.renameError'));
      }
    } catch {
      notify.error(t('snapshots.renameError'));
    } finally {
      setRenameTarget(null);
      setRenameName('');
    }
  };

  const handleShareConfirm = useCallback(
    async (config: ShareExportDialogRequest) => {
      if (!shareTarget) return;

      const shareUrl = await shareSnapshot({
        snapshot_id: shareTarget.id,
        format: config.format,
        include_headers: config.include_headers,
        table_name: config.table_name,
        limit: config.limit,
        file_name: config.file_name,
      });

      if (shareUrl) {
        setShareTarget(null);
      }
    },
    [shareSnapshot, shareTarget]
  );

  const filtered = search.trim()
    ? snapshots.filter(
        s =>
          s.name.toLowerCase().includes(search.toLowerCase()) ||
          s.source.toLowerCase().includes(search.toLowerCase()) ||
          s.connection_name?.toLowerCase().includes(search.toLowerCase())
      )
    : snapshots;

  if (previewSnapshot) {
    return (
      <div className="flex-1 flex flex-col min-h-0">
        {/* Preview header */}
        <div className="shrink-0 flex items-center justify-between px-4 py-3 border-b border-border bg-muted/10">
          <div className="flex items-center gap-3 min-w-0">
            <Button
              variant="ghost"
              size="sm"
              className="h-7"
              onClick={() => setPreviewSnapshot(null)}
            >
              <X size={14} className="mr-1" />
              {t('common.back')}
            </Button>
            <div className="h-4 w-px bg-border" />
            <Camera size={16} className="text-accent shrink-0" />
            <div className="min-w-0">
              <h3 className="text-sm font-medium truncate">{previewSnapshot.meta.name}</h3>
              <p className="text-xs text-muted-foreground truncate">
                {previewSnapshot.meta.source_type === 'table' ? (
                  <span className="inline-flex items-center gap-1">
                    <Table2 size={10} /> {previewSnapshot.meta.source}
                  </span>
                ) : (
                  <span className="font-mono">{previewSnapshot.meta.source.slice(0, 60)}</span>
                )}
                {' · '}
                {t('snapshots.rows', { count: previewSnapshot.meta.row_count })}
                {' · '}
                {formatRelativeTime(previewSnapshot.meta.created_at)}
              </p>
            </div>
          </div>
          <div className="flex items-center gap-2">
            <Button
              variant="outline"
              size="sm"
              className="h-7 gap-1.5"
              onClick={() => setShareTarget(previewSnapshot.meta)}
            >
              <Link2 size={14} />
              {t('share.generateLink')}
            </Button>
            {onCompareInDiff && (
              <Button
                variant="outline"
                size="sm"
                className="h-7 gap-1.5"
                onClick={() => onCompareInDiff(previewSnapshot.meta.id, previewSnapshot.meta)}
              >
                <GitCompare size={14} />
                {t('snapshots.compareWithDiff')}
              </Button>
            )}
          </div>
        </div>
        {/* Preview grid */}
        <div className="flex-1 min-h-0 p-2 flex flex-col">
          <DataGrid result={previewSnapshot.result} readOnly />
        </div>

        {shareTarget && (
          <ShareExportDialog
            open={true}
            onOpenChange={open => {
              if (!open) setShareTarget(null);
            }}
            defaultFileName={shareTarget.name}
            defaultTableName={
              shareTarget.source_type === 'table' ? shareTarget.source : undefined
            }
            showBatchSize={false}
            onConfirm={handleShareConfirm}
          />
        )}
      </div>
    );
  }

  return (
    <div className="flex-1 flex flex-col min-h-0">
      {/* Header */}
      <div className="shrink-0 flex items-center justify-between px-4 py-3 border-b border-border bg-muted/10">
        <div className="flex items-center gap-2">
          <Camera size={18} className="text-accent" />
          <h2 className="text-sm font-semibold">{t('snapshots.title')}</h2>
          {!loading && (
            <span className="text-xs text-muted-foreground ml-1">({snapshots.length})</span>
          )}
        </div>
        <div className="relative">
          <Search
            size={14}
            className="absolute left-2 top-1/2 -translate-y-1/2 text-muted-foreground"
          />
          <Input
            type="text"
            placeholder={t('common.search')}
            value={search}
            onChange={e => setSearch(e.target.value)}
            className="h-7 w-48 pl-7 text-xs"
          />
        </div>
      </div>

      {/* Content */}
      <div className="flex-1 min-h-0 overflow-y-auto">
        {loading ? (
          <div className="flex items-center justify-center h-40">
            <Loader2 size={20} className="animate-spin text-muted-foreground" />
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-60 gap-3 text-center px-8">
            <Camera size={40} className="text-muted-foreground/30" />
            <div>
              <p className="text-sm text-muted-foreground">{t('snapshots.noSnapshots')}</p>
              <p className="text-xs text-muted-foreground/70 mt-1">
                {t('snapshots.noSnapshotsHint')}
              </p>
            </div>
          </div>
        ) : (
          <div className="p-4 grid gap-3">
            {filtered.map(snap => (
              <button
                type="button"
                key={snap.id}
                className={cn(
                  'group border border-border rounded-lg p-4 bg-card hover:bg-muted/30 transition-colors cursor-pointer text-left w-full',
                  previewLoading && 'opacity-50 pointer-events-none'
                )}
                onClick={() => handlePreview(snap)}
              >
                <div className="flex items-start justify-between gap-3">
                  <div className="min-w-0 flex-1">
                    <div className="flex items-center gap-2">
                      <h3 className="text-sm font-medium truncate">{snap.name}</h3>
                      {snap.driver && (
                        <span className="text-[10px] uppercase px-1.5 py-0.5 rounded bg-muted text-muted-foreground font-medium">
                          {snap.driver}
                        </span>
                      )}
                    </div>
                    {snap.description && (
                      <p className="text-xs text-muted-foreground mt-1 line-clamp-2">
                        {snap.description}
                      </p>
                    )}
                    <div className="flex items-center gap-3 mt-2 text-xs text-muted-foreground flex-wrap">
                      <span className="inline-flex items-center gap-1">
                        {snap.source_type === 'table' ? (
                          <Table2 size={11} />
                        ) : (
                          <Database size={11} />
                        )}
                        <span className="font-mono truncate max-w-48">
                          {snap.source.length > 50 ? `${snap.source.slice(0, 50)}...` : snap.source}
                        </span>
                      </span>
                      <span className="inline-flex items-center gap-1">
                        <HardDrive size={11} />
                        {formatFileSize(snap.file_size)}
                      </span>
                      <span>
                        {t('snapshots.rows', { count: snap.row_count })} &middot;{' '}
                        {snap.columns.length} col.
                      </span>
                      <span className="inline-flex items-center gap-1">
                        <Calendar size={11} />
                        {formatRelativeTime(snap.created_at)}
                      </span>
                      {snap.connection_name && (
                        <span className="truncate max-w-32">{snap.connection_name}</span>
                      )}
                    </div>
                  </div>
                  {/* Actions */}
                  <div className="flex items-center gap-1 opacity-0 group-hover:opacity-100 transition-opacity shrink-0">
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      title={t('snapshots.preview')}
                      onClick={e => {
                        e.stopPropagation();
                        handlePreview(snap);
                      }}
                    >
                      <Eye size={14} />
                    </Button>
                    {onCompareInDiff && (
                      <Button
                        variant="ghost"
                        size="icon"
                        className="h-7 w-7"
                        title={t('snapshots.compareWithDiff')}
                        onClick={e => {
                          e.stopPropagation();
                          onCompareInDiff(snap.id, snap);
                        }}
                      >
                        <GitCompare size={14} />
                      </Button>
                    )}
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      title={t('share.generateLink')}
                      onClick={e => {
                        e.stopPropagation();
                        setShareTarget(snap);
                      }}
                    >
                      <Link2 size={14} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7"
                      title={t('snapshots.rename')}
                      onClick={e => {
                        e.stopPropagation();
                        setRenameTarget(snap);
                        setRenameName(snap.name);
                      }}
                    >
                      <Pencil size={14} />
                    </Button>
                    <Button
                      variant="ghost"
                      size="icon"
                      className="h-7 w-7 text-destructive hover:text-destructive"
                      title={t('snapshots.delete')}
                      onClick={e => {
                        e.stopPropagation();
                        setDeleteTarget(snap);
                      }}
                    >
                      <Trash2 size={14} />
                    </Button>
                  </div>
                </div>
              </button>
            ))}
          </div>
        )}
      </div>

      {shareTarget && (
        <ShareExportDialog
          open={true}
          onOpenChange={open => {
            if (!open) setShareTarget(null);
          }}
          defaultFileName={shareTarget.name}
          defaultTableName={shareTarget.source_type === 'table' ? shareTarget.source : undefined}
          showBatchSize={false}
          onConfirm={handleShareConfirm}
        />
      )}

      {/* Rename Dialog */}
      <Dialog open={!!renameTarget} onOpenChange={open => !open && setRenameTarget(null)}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle>{t('snapshots.renameTitle')}</DialogTitle>
          </DialogHeader>
          <Input
            value={renameName}
            onChange={e => setRenameName(e.target.value)}
            autoFocus
            onKeyDown={e => {
              if (e.key === 'Enter' && renameName.trim()) handleRename();
            }}
          />
          <DialogFooter>
            <Button variant="outline" onClick={() => setRenameTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button onClick={handleRename} disabled={!renameName.trim()}>
              {t('snapshots.rename')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {/* Delete Confirmation Dialog */}
      <Dialog open={!!deleteTarget} onOpenChange={open => !open && setDeleteTarget(null)}>
        <DialogContent className="sm:max-w-sm">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <AlertCircle size={18} className="text-destructive" />
              {t('snapshots.deleteTitle')}
            </DialogTitle>
          </DialogHeader>
          <p className="text-sm text-muted-foreground">
            {t('snapshots.deleteConfirm', { name: deleteTarget?.name })}
          </p>
          <DialogFooter>
            <Button variant="outline" onClick={() => setDeleteTarget(null)}>
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleDelete}>
              {t('snapshots.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>
    </div>
  );
}

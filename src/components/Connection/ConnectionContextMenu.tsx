// SPDX-License-Identifier: Apache-2.0

import {
  BookOpen,
  Copy,
  Download,
  Eraser,
  FileCode,
  Loader2,
  Pencil,
  Star,
  Terminal,
  Trash2,
  Upload,
  Wrench,
  Zap,
} from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuSub,
  ContextMenuSubContent,
  ContextMenuSubTrigger,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import { openBackupDialog, openImportSqlDialog, openRestoreDialog } from '@/lib/stores/modalStore';
import { CONNECTION_BACKUP_DRIVERS } from '@/lib/tauri/backup';
import { TRUNCATE_ALL_DRIVERS, truncateAll } from '@/lib/tauri/maintenance';
import type { SavedConnection } from '../../lib/tauri';
import { DangerConfirmDialog } from '../Guard/DangerConfirmDialog';
import { useConnectionActions } from './useConnectionActions';

interface ConnectionContextMenuProps {
  connection: SavedConnection;
  sessionId?: string;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  isFavorite?: boolean;
  onToggleFavorite?: () => void;
  onNewQuery?: () => void;
  onNewNotebook?: () => void;
  isConnected?: boolean;
  children: ReactNode;
}

export function ConnectionContextMenu({
  connection,
  sessionId,
  onEdit,
  onDeleted,
  isFavorite,
  onToggleFavorite,
  onNewQuery,
  onNewNotebook,
  isConnected,
  children,
}: ConnectionContextMenuProps) {
  const { t } = useTranslation();
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const [truncateOpen, setTruncateOpen] = useState(false);
  const [truncateLoading, setTruncateLoading] = useState(false);
  const { testing, deleting, duplicating, handleTest, handleEdit, handleDelete, handleDuplicate } =
    useConnectionActions({
      connection,
      onEdit,
      onDeleted,
      onAfterAction: () => setShowDeleteConfirm(false),
    });

  const driverId = connection.driver.toLowerCase();
  const canTruncateAll =
    !!isConnected &&
    !!sessionId &&
    !connection.read_only &&
    CONNECTION_BACKUP_DRIVERS.has(driverId) &&
    TRUNCATE_ALL_DRIVERS.has(driverId);
  const canImportSql =
    !!isConnected &&
    !!sessionId &&
    !connection.read_only &&
    CONNECTION_BACKUP_DRIVERS.has(driverId);

  async function handleTruncateAll() {
    if (!sessionId) return;
    setTruncateLoading(true);
    try {
      const result = await truncateAll(sessionId, connection.database ?? '', null, true);
      if (result.success) {
        toast.success(
          t('truncateAll.success', { count: result.result?.truncated_tables.length ?? 0 })
        );
        setTruncateOpen(false);
      } else {
        toast.error(result.error || t('truncateAll.failed'));
      }
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setTruncateLoading(false);
    }
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          {isConnected && (onNewQuery || onNewNotebook) && (
            <>
              {onNewQuery && (
                <ContextMenuItem onSelect={() => onNewQuery()}>
                  <Terminal size={14} />
                  {t('connection.menu.newQuery')}
                </ContextMenuItem>
              )}
              {onNewNotebook && (
                <ContextMenuItem onSelect={() => onNewNotebook()}>
                  <BookOpen size={14} />
                  {t('connection.menu.newNotebook')}
                </ContextMenuItem>
              )}
              <ContextMenuSeparator />
            </>
          )}
          <ContextMenuItem onSelect={() => handleTest()} disabled={testing}>
            {testing ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
            {t('connection.menu.testConnection')}
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => handleEdit()}>
            <Pencil size={14} />
            {t('connection.menu.edit')}
          </ContextMenuItem>
          <ContextMenuItem onSelect={() => handleDuplicate()} disabled={duplicating}>
            {duplicating ? <Loader2 size={14} className="animate-spin" /> : <Copy size={14} />}
            {t('connection.menu.duplicate')}
          </ContextMenuItem>
          {onToggleFavorite && (
            <ContextMenuItem onSelect={() => onToggleFavorite()}>
              <Star size={14} className={isFavorite ? 'fill-current text-yellow-500' : ''} />
              {isFavorite ? t('sidebar.removeFromFavorites') : t('sidebar.addToFavorites')}
            </ContextMenuItem>
          )}
          {CONNECTION_BACKUP_DRIVERS.has(connection.driver.toLowerCase()) && (
            <>
              <ContextMenuSeparator />
              <ContextMenuSub>
                <ContextMenuSubTrigger>
                  <Wrench size={14} />
                  {t('dbtree.tools')}
                </ContextMenuSubTrigger>
                <ContextMenuSubContent className="w-48">
                  <ContextMenuItem onSelect={() => openBackupDialog(connection)}>
                    <Download size={14} />
                    {t('connection.menu.backup')}
                  </ContextMenuItem>
                  <ContextMenuItem onSelect={() => openRestoreDialog(connection)}>
                    <Upload size={14} />
                    {t('connection.menu.restore')}
                  </ContextMenuItem>
                  {canImportSql && sessionId && (
                    <ContextMenuItem
                      onSelect={() =>
                        openImportSqlDialog({
                          sessionId,
                          database: connection.database ?? '',
                          schema: null,
                          label: connection.database || connection.name,
                        })
                      }
                    >
                      <FileCode size={14} />
                      {t('importSql.menuItem')}
                    </ContextMenuItem>
                  )}
                </ContextMenuSubContent>
              </ContextMenuSub>
            </>
          )}
          <ContextMenuSeparator />
          {canTruncateAll && (
            <ContextMenuItem
              onSelect={e => {
                e.preventDefault();
                setTruncateOpen(true);
              }}
              className="text-destructive focus:text-destructive focus:bg-destructive/10"
            >
              <Eraser size={14} />
              {t('truncateAll.menuItem')}
            </ContextMenuItem>
          )}
          <ContextMenuItem
            variant="destructive"
            onSelect={e => {
              e.preventDefault();
              setShowDeleteConfirm(true);
            }}
            disabled={deleting}
          >
            {deleting ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
            {t('connection.menu.delete')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <Dialog open={showDeleteConfirm} onOpenChange={setShowDeleteConfirm}>
        <DialogContent>
          <DialogHeader>
            <DialogTitle>{t('connection.menu.delete')}</DialogTitle>
          </DialogHeader>
          <div className="py-4">
            <p className="text-sm text-muted-foreground">
              {t('connection.menu.deleteConfirm', { name: connection.name })}
            </p>
          </div>
          <DialogFooter>
            <Button
              variant="outline"
              onClick={() => setShowDeleteConfirm(false)}
              disabled={deleting}
            >
              {t('common.cancel')}
            </Button>
            <Button variant="destructive" onClick={handleDelete} disabled={deleting}>
              {deleting ? (
                <Loader2 size={14} className="animate-spin mr-2" />
              ) : (
                <Trash2 size={14} className="mr-2" />
              )}
              {t('common.delete')}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      <DangerConfirmDialog
        open={truncateOpen}
        onOpenChange={setTruncateOpen}
        title={t('truncateAll.menuItem')}
        description={t('truncateAll.description', {
          name: connection.database || connection.name,
        })}
        confirmationLabel={connection.database || connection.name}
        confirmLabel={t('truncateAll.confirm')}
        loading={truncateLoading}
        onConfirm={handleTruncateAll}
      />
    </>
  );
}

// SPDX-License-Identifier: Apache-2.0

import { Copy, Loader2, Pencil, Trash2, Zap } from 'lucide-react';
import { type ReactNode, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { SavedConnection } from '../../lib/tauri';
import { useConnectionActions } from './useConnectionActions';

interface ConnectionContextMenuProps {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  isFavorite?: boolean;
  onToggleFavorite?: () => void;
  children: ReactNode;
}

export function ConnectionContextMenu({
  connection,
  onEdit,
  onDeleted,
  isFavorite,
  onToggleFavorite,
  children,
}: ConnectionContextMenuProps) {
  const { t } = useTranslation();
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const { testing, deleting, duplicating, handleTest, handleEdit, handleDelete, handleDuplicate } =
    useConnectionActions({
      connection,
      onEdit,
      onDeleted,
      onAfterAction: () => setShowDeleteConfirm(false),
    });

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-48">
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
              {isFavorite ? t('sidebar.removeFromFavorites') : t('sidebar.addToFavorites')}
            </ContextMenuItem>
          )}
          <ContextMenuSeparator />
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
    </>
  );
}

// SPDX-License-Identifier: Apache-2.0

import {
  BookOpen,
  Copy,
  Loader2,
  MoreVertical,
  Pencil,
  Star,
  Terminal,
  Trash2,
  Zap,
} from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import type { SavedConnection } from '../../lib/tauri';
import { useConnectionActions } from './useConnectionActions';

interface ConnectionMenuProps {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  isFavorite?: boolean;
  onToggleFavorite?: () => void;
  onNewQuery?: () => void;
  onNewNotebook?: () => void;
  isConnected?: boolean;
}

export function ConnectionMenu({
  connection,
  onEdit,
  onDeleted,
  isFavorite,
  onToggleFavorite,
  onNewQuery,
  onNewNotebook,
  isConnected,
}: ConnectionMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const { t } = useTranslation();

  const { testing, deleting, duplicating, handleTest, handleEdit, handleDelete, handleDuplicate } =
    useConnectionActions({
      connection,
      onEdit,
      onDeleted,
      onAfterAction: () => {
        setIsOpen(false);
        setShowDeleteConfirm(false);
      },
    });

  return (
    <>
      <DropdownMenu open={isOpen} onOpenChange={setIsOpen}>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-6 w-6"
            onClick={e => e.stopPropagation()}
          >
            <MoreVertical size={14} />
          </Button>
        </DropdownMenuTrigger>

        <DropdownMenuContent
          align="end"
          className="whitespace-nowrap"
          onClick={e => e.stopPropagation()}
        >
          {isConnected && (onNewQuery || onNewNotebook) && (
            <>
              {onNewQuery && (
                <DropdownMenuItem onSelect={onNewQuery}>
                  <Terminal size={14} />
                  {t('connection.menu.newQuery')}
                </DropdownMenuItem>
              )}
              {onNewNotebook && (
                <DropdownMenuItem onSelect={onNewNotebook}>
                  <BookOpen size={14} />
                  {t('connection.menu.newNotebook')}
                </DropdownMenuItem>
              )}
              <DropdownMenuSeparator />
            </>
          )}

          <DropdownMenuItem
            onSelect={event => {
              event.preventDefault();
              handleTest();
            }}
            disabled={testing}
          >
            {testing ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
            {t('connection.menu.testConnection')}
          </DropdownMenuItem>

          <DropdownMenuItem
            onSelect={event => {
              event.preventDefault();
              handleEdit();
            }}
          >
            <Pencil size={14} />
            {t('connection.menu.edit')}
          </DropdownMenuItem>

          <DropdownMenuItem
            onSelect={event => {
              event.preventDefault();
              handleDuplicate();
            }}
            disabled={duplicating}
          >
            {duplicating ? <Loader2 size={14} className="animate-spin" /> : <Copy size={14} />}
            {t('connection.menu.duplicate')}
          </DropdownMenuItem>

          {onToggleFavorite && (
            <DropdownMenuItem onSelect={onToggleFavorite}>
              <Star size={14} className={isFavorite ? 'fill-current text-yellow-500' : ''} />
              {isFavorite ? t('sidebar.removeFromFavorites') : t('sidebar.addToFavorites')}
            </DropdownMenuItem>
          )}

          <DropdownMenuSeparator />

          <DropdownMenuItem
            variant="destructive"
            onSelect={event => {
              event.preventDefault();
              setIsOpen(false);
              setShowDeleteConfirm(true);
            }}
            disabled={deleting}
          >
            {deleting ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
            {t('connection.menu.delete')}
          </DropdownMenuItem>
        </DropdownMenuContent>
      </DropdownMenu>

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

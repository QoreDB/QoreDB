// SPDX-License-Identifier: Apache-2.0

import { Copy, Loader2, MoreVertical, Pencil, Star, Trash2, Zap } from 'lucide-react';
import { useEffect, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  Dialog,
  DialogContent,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from '@/components/ui/dialog';
import type { SavedConnection } from '../../lib/tauri';
import { useConnectionActions } from './useConnectionActions';

interface ConnectionMenuProps {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  isFavorite?: boolean;
  onToggleFavorite?: () => void;
}

export function ConnectionMenu({
  connection,
  onEdit,
  onDeleted,
  isFavorite,
  onToggleFavorite,
}: ConnectionMenuProps) {
  const [isOpen, setIsOpen] = useState(false);
  const [showDeleteConfirm, setShowDeleteConfirm] = useState(false);
  const menuRef = useRef<HTMLDivElement>(null);
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

  useEffect(() => {
    function handleClickOutside(e: MouseEvent) {
      if (menuRef.current && !menuRef.current.contains(e.target as Node)) {
        setIsOpen(false);
      }
    }
    if (isOpen) {
      document.addEventListener('mousedown', handleClickOutside);
      return () => document.removeEventListener('mousedown', handleClickOutside);
    }
  }, [isOpen]);

  const onSelectDelete = (e: React.MouseEvent) => {
    e.stopPropagation();
    setIsOpen(false);
    setShowDeleteConfirm(true);
  };

  return (
    <>
      <div className="relative" ref={menuRef}>
        <Button
          variant="ghost"
          size="icon"
          className="h-6 w-6"
          onClick={e => {
            e.stopPropagation();
            setIsOpen(!isOpen);
          }}
        >
          <MoreVertical size={14} />
        </Button>

        {isOpen && (
          <div
            className="absolute right-0 top-full mt-1 z-50 min-w-40 bg-background border border-border rounded-md shadow-lg py-1 animate-in fade-in-0 zoom-in-95"
            onClick={e => e.stopPropagation()}
          >
            <button
              className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
              onClick={handleTest}
              disabled={testing}
            >
              {testing ? <Loader2 size={14} className="animate-spin" /> : <Zap size={14} />}
              {t('connection.menu.testConnection')}
            </button>

            <button
              className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
              onClick={handleEdit}
            >
              <Pencil size={14} />
              {t('connection.menu.edit')}
            </button>

            <button
              className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
              onClick={handleDuplicate}
              disabled={duplicating}
            >
              {duplicating ? <Loader2 size={14} className="animate-spin" /> : <Copy size={14} />}
              {t('connection.menu.duplicate')}
            </button>

            {onToggleFavorite && (
              <button
                className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-muted transition-colors text-left"
                onClick={() => {
                  onToggleFavorite();
                  setIsOpen(false);
                }}
              >
                <Star size={14} className={isFavorite ? 'fill-current text-yellow-500' : ''} />
                {isFavorite ? t('sidebar.removeFromFavorites') : t('sidebar.addToFavorites')}
              </button>
            )}

            <div className="h-px bg-border my-1" />

            <button
              className="w-full flex items-center gap-2 px-3 py-1.5 text-sm hover:bg-error/10 text-error transition-colors text-left"
              onClick={onSelectDelete}
              disabled={deleting}
            >
              {deleting ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
              {t('connection.menu.delete')}
            </button>
          </div>
        )}
      </div>

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

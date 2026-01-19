import { ReactNode } from 'react';
import { Copy, Loader2, Pencil, Trash2, Zap } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { SavedConnection } from '../../lib/tauri';
import { useConnectionActions } from './useConnectionActions';
import { useTranslation } from 'react-i18next';

interface ConnectionContextMenuProps {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  children: ReactNode;
}

export function ConnectionContextMenu({
  connection,
  onEdit,
  onDeleted,
  children,
}: ConnectionContextMenuProps) {
  const { t } = useTranslation();
  const {
    testing,
    deleting,
    duplicating,
    handleTest,
    handleEdit,
    handleDelete,
    handleDuplicate,
  } = useConnectionActions({
    connection,
    onEdit,
    onDeleted,
  });

  return (
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
        <ContextMenuSeparator />
        <ContextMenuItem
          variant="destructive"
          onSelect={() => handleDelete()}
          disabled={deleting}
        >
          {deleting ? <Loader2 size={14} className="animate-spin" /> : <Trash2 size={14} />}
          {t('connection.menu.delete')}
        </ContextMenuItem>
      </ContextMenuContent>
    </ContextMenu>
  );
}

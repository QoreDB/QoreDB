import { ReactNode } from 'react';
import { Database, Plus, RefreshCw, Trash2 } from 'lucide-react';
import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { useTranslation } from 'react-i18next';

interface DatabaseContextMenuProps {
  onOpen: () => void;
  onRefresh: () => void;
  onCreateTable?: () => void;
  onDelete?: () => void;
  canCreateTable: boolean;
  canDelete: boolean;
  children: ReactNode;
}

export function DatabaseContextMenu({
  onOpen,
  onRefresh,
  onCreateTable,
  onDelete,
  canCreateTable,
  canDelete,
  children,
}: DatabaseContextMenuProps) {
  const { t } = useTranslation();

  return (
    <ContextMenu>
      <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
      <ContextMenuContent className="w-48">
        <ContextMenuItem onSelect={onOpen}>
          <Database size={14} />
          {t('dbtree.open')}
        </ContextMenuItem>
        <ContextMenuItem onSelect={onRefresh}>
          <RefreshCw size={14} />
          {t('dbtree.refresh')}
        </ContextMenuItem>
        {canCreateTable && onCreateTable && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={onCreateTable}>
              <Plus size={14} />
              {t('createTable.title')}
            </ContextMenuItem>
          </>
        )}
        {canDelete && onDelete && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem
              onSelect={onDelete}
              className="text-destructive focus:text-destructive focus:bg-destructive/10"
            >
              <Trash2 size={14} />
              {t('dropDatabase.menuItem')}
            </ContextMenuItem>
          </>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}

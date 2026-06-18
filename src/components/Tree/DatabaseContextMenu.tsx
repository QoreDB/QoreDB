// SPDX-License-Identifier: Apache-2.0

import { Database, Download, HardDriveDownload, Plus, RefreshCw, Trash2 } from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';

interface DatabaseContextMenuProps {
  onOpen: () => void;
  onRefresh: () => void;
  onCreateTable?: () => void;
  onDelete?: () => void;
  onExportSchema?: () => void;
  onExportDatabase?: () => void;
  canCreateTable: boolean;
  canDelete: boolean;
  canExportSchema: boolean;
  canExportDatabase: boolean;
  children: ReactNode;
}

export function DatabaseContextMenu({
  onOpen,
  onRefresh,
  onCreateTable,
  onDelete,
  onExportSchema,
  onExportDatabase,
  canCreateTable,
  canDelete,
  canExportSchema,
  canExportDatabase,
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
        {canExportSchema && onExportSchema && (
          <>
            <ContextMenuSeparator />
            <ContextMenuItem onSelect={onExportSchema}>
              <Download size={14} />
              {t('schemaExport.menuItem')}
            </ContextMenuItem>
            {canExportDatabase && onExportDatabase && (
              <ContextMenuItem onSelect={onExportDatabase}>
                <HardDriveDownload size={14} />
                {t('databaseExport.menuItem')}
              </ContextMenuItem>
            )}
          </>
        )}
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

// SPDX-License-Identifier: Apache-2.0

import {
  Database,
  Download,
  Eraser,
  FileCode,
  Plus,
  RefreshCw,
  Trash2,
  Upload,
  Wrench,
} from 'lucide-react';
import type { ReactNode } from 'react';
import { useTranslation } from 'react-i18next';

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

interface DatabaseContextMenuProps {
  onOpen: () => void;
  onRefresh: () => void;
  onCreateTable?: () => void;
  onDelete?: () => void;
  onExportSchema?: () => void;
  onBackup?: () => void;
  onRestore?: () => void;
  onImportSql?: () => void;
  onTruncateAll?: () => void;
  canCreateTable: boolean;
  canDelete: boolean;
  canExportSchema: boolean;
  canBackup: boolean;
  canImportSql: boolean;
  canTruncateAll: boolean;
  isDocument?: boolean;
  children: ReactNode;
}

export function DatabaseContextMenu({
  onOpen,
  onRefresh,
  onCreateTable,
  onDelete,
  onExportSchema,
  onBackup,
  onRestore,
  onImportSql,
  onTruncateAll,
  canCreateTable,
  canDelete,
  canExportSchema,
  canBackup,
  canImportSql,
  canTruncateAll,
  isDocument,
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
        {canBackup && onBackup && onRestore ? (
          <>
            <ContextMenuSeparator />
            <ContextMenuSub>
              <ContextMenuSubTrigger>
                <Wrench size={14} />
                {t('dbtree.tools')}
              </ContextMenuSubTrigger>
              <ContextMenuSubContent className="w-48">
                {canExportSchema && onExportSchema && (
                  <ContextMenuItem onSelect={onExportSchema}>
                    <Download size={14} />
                    {t('schemaExport.menuItem')}
                  </ContextMenuItem>
                )}
                <ContextMenuItem onSelect={onBackup}>
                  <Download size={14} />
                  {t('connection.menu.backup')}
                </ContextMenuItem>
                <ContextMenuItem onSelect={onRestore}>
                  <Upload size={14} />
                  {t('connection.menu.restore')}
                </ContextMenuItem>
                {canImportSql && onImportSql && (
                  <ContextMenuItem onSelect={onImportSql}>
                    <FileCode size={14} />
                    {t('importSql.menuItem')}
                  </ContextMenuItem>
                )}
              </ContextMenuSubContent>
            </ContextMenuSub>
          </>
        ) : (
          canExportSchema &&
          onExportSchema && (
            <>
              <ContextMenuSeparator />
              <ContextMenuItem onSelect={onExportSchema}>
                <Download size={14} />
                {t('schemaExport.menuItem')}
              </ContextMenuItem>
            </>
          )
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
        {((canTruncateAll && onTruncateAll) || (canDelete && onDelete)) && <ContextMenuSeparator />}
        {canTruncateAll && onTruncateAll && (
          <ContextMenuItem
            onSelect={onTruncateAll}
            className="text-destructive focus:text-destructive focus:bg-destructive/10"
          >
            <Eraser size={14} />
            {t(isDocument ? 'truncateAll.menuItemDocument' : 'truncateAll.menuItem')}
          </ContextMenuItem>
        )}
        {canDelete && onDelete && (
          <ContextMenuItem
            onSelect={onDelete}
            className="text-destructive focus:text-destructive focus:bg-destructive/10"
          >
            <Trash2 size={14} />
            {t('dropDatabase.menuItem')}
          </ContextMenuItem>
        )}
      </ContextMenuContent>
    </ContextMenu>
  );
}

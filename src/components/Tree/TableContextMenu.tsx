import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Eye, Trash2, Eraser } from 'lucide-react';
import { notify } from '../../lib/notify';

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';
import { Collection, Environment, executeQuery } from '../../lib/tauri';
import { Driver, getDriverMetadata } from '../../lib/drivers';
import { buildDropTableSQL, buildTruncateTableSQL } from '@/lib/column-types';
import { emitTableChange } from '@/lib/tableEvents';
import { invalidateCollectionsCache, invalidateTableSchemaCache } from '../../hooks/useSchemaCache';

interface TableContextMenuProps {
  collection: Collection;
  sessionId: string;
  driver: Driver;
  environment: Environment;
  readOnly: boolean;
  rowCountEstimate?: number;
  onRefresh: () => void;
  onOpen: () => void;
  children: React.ReactNode;
}

type DangerAction = 'drop' | 'truncate' | null;

/**
 * Right-click context menu wrapper for table items.
 * Wraps children and provides native context menu on right-click.
 */
export function TableContextMenu({
  collection,
  sessionId,
  driver,
  environment,
  readOnly,
  rowCountEstimate,
  onRefresh,
  onOpen,
  children,
}: TableContextMenuProps) {
  const { t } = useTranslation();
  const [dangerAction, setDangerAction] = useState<DangerAction>(null);
  const [loading, setLoading] = useState(false);
  
  const driverMeta = getDriverMetadata(driver);
  const isProduction = environment === 'production';
  const isMongo = !driverMeta.supportsSQL;
  const tableName = collection.name;
  const confirmationLabel = isProduction ? tableName : undefined;

  async function handleDropTable() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      let query: string;

      if (isMongo) {
        const payload = {
          database: collection.namespace.database,
          collection: tableName,
          operation: 'drop_collection',
        };
        query = JSON.stringify(payload);
      } else {
        const schemaOrDb = collection.namespace.schema || collection.namespace.database;
        query = buildDropTableSQL(schemaOrDb, tableName, driver);
      }

      const result = await executeQuery(sessionId, query, {
        acknowledgedDangerous: true,
      });

      if (result.success) {
        // Invalidate cache before refresh
        invalidateCollectionsCache(sessionId, collection.namespace);
        invalidateTableSchemaCache(sessionId, collection.namespace, tableName);
        notify.success(t('dropTable.success', { name: tableName }));
        onRefresh();
        setDangerAction(null);
        emitTableChange({ type: 'drop', namespace: collection.namespace, tableName });
      } else {
        notify.error(t('dropTable.failed'), result.error);
      }
    } catch (err) {
      notify.error(t('common.error'), err);
    } finally {
      setLoading(false);
    }
  }

  async function handleTruncateTable() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      let query: string;

      if (isMongo) {
        const payload = {
          database: collection.namespace.database,
          collection: tableName,
          operation: 'delete_many',
          filter: {},
        };
        query = JSON.stringify(payload);
      } else {
        query = buildTruncateTableSQL(collection.namespace, tableName, driver);
      }

      const result = await executeQuery(sessionId, query, {
        acknowledgedDangerous: true,
      });

      if (result.success) {
        // Invalidate table schema cache (data changed, schema may have stats)
        invalidateTableSchemaCache(sessionId, collection.namespace, tableName);
        notify.success(t('tableMenu.truncateSuccess', { name: tableName }));
        onRefresh();
        setDangerAction(null);
        emitTableChange({ type: 'truncate', namespace: collection.namespace, tableName });
      } else {
        notify.error(t('tableMenu.truncateError'), result.error);
      }
    } catch (err) {
      notify.error(t('common.error'), err);
    } finally {
      setLoading(false);
    }
  }

  function getWarningInfo(): string | undefined {
    if (rowCountEstimate !== undefined && rowCountEstimate > 0) {
      return t('tableMenu.rowsWillBeDeleted', { count: rowCountEstimate });
    }
    return undefined;
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>
          {children}
        </ContextMenuTrigger>
        <ContextMenuContent className="w-44">
          <ContextMenuItem onClick={onOpen}>
            <Eye size={14} className="mr-2" />
            {t('tableMenu.open')}
          </ContextMenuItem>
          
          <ContextMenuSeparator />
          
          <ContextMenuItem
            onClick={() => setDangerAction('truncate')}
            disabled={readOnly}
            className="text-warning focus:text-warning"
          >
            <Eraser size={14} className="mr-2" />
            {t('tableMenu.truncate')}
          </ContextMenuItem>
          
          <ContextMenuItem
            onClick={() => setDangerAction('drop')}
            disabled={readOnly}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 size={14} className="mr-2" />
            {t('tableMenu.drop')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      {/* Drop Table Confirmation */}
      <DangerConfirmDialog
        open={dangerAction === 'drop'}
        onOpenChange={(open) => !open && setDangerAction(null)}
        title={t('dropTable.title')}
        description={t('dropTable.confirm', { name: tableName })}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.delete')}
        loading={loading}
        onConfirm={handleDropTable}
      />

      {/* Truncate Table Confirmation */}
      <DangerConfirmDialog
        open={dangerAction === 'truncate'}
        onOpenChange={(open) => !open && setDangerAction(null)}
        title={t('tableMenu.truncateTitle')}
        description={t('tableMenu.truncateDescription', { name: tableName })}
        confirmationLabel={confirmationLabel}
        warningInfo={getWarningInfo()}
        confirmLabel={t('tableMenu.truncateConfirm')}
        loading={loading}
        onConfirm={handleTruncateTable}
      />
    </>
  );
}

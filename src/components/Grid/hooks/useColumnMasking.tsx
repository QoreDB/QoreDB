// SPDX-License-Identifier: BUSL-1.1

import { useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { ProductionConfirmDialog } from '@/components/Guard/ProductionConfirmDialog';
import {
  EMPTY_MASKING,
  findRule,
  setConnectionMasking,
  withoutRule,
  withRule,
} from '@/lib/masking';
import { captureWorkspaceScope } from '@/lib/stores/workspaceStore';
import type { ConnectionMasking, Environment } from '@/lib/tauri';
import { useLicense } from '@/providers/LicenseProvider';
import { useSessionContext } from '@/providers/SessionProvider';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import type { ColumnMaskMenu } from '../DataGridTableHeader';

interface UseColumnMaskingOptions {
  connectionId?: string;
  /** Undefined for free query results: the rule then applies to any table. */
  tableName?: string;
  environment: Environment;
  confirmationLabel: string;
  maskedColumns: Set<string>;
  /** Reloads the data; without it the change applies from the next run. */
  onChanged?: () => void;
}

export function useColumnMasking({
  connectionId,
  tableName,
  environment,
  confirmationLabel,
  maskedColumns,
  onChanged,
}: UseColumnMaskingOptions) {
  const { t } = useTranslation();
  const { savedConnections, refreshSidebar } = useSessionContext();
  const { projectId } = useWorkspace();
  const { isFeatureEnabled } = useLicense();
  const savedConnection = savedConnections.find(connection => connection.id === connectionId);
  const [pendingRemoval, setPendingRemoval] = useState<(() => void) | null>(null);
  const context = useMemo(
    () => ({ projectId, connectionId, tableName, savedConnection }),
    [projectId, connectionId, tableName, savedConnection]
  );
  const current = useRef<typeof context | null>(context);
  current.current = context;
  const busy = useRef(false);
  useEffect(() => {
    current.current = context;
    setPendingRemoval(null);
    return () => {
      current.current = null;
    };
  }, [context]);
  function captureContext() {
    const inWorkspace = captureWorkspaceScope(projectId);
    return () => current.current === context && inWorkspace();
  }

  const masking = savedConnection?.masking ?? EMPTY_MASKING;

  async function save(next: ConnectionMasking, message: string, isCurrent = captureContext()) {
    if (!connectionId || !savedConnection || !isCurrent() || busy.current) return;
    busy.current = true;
    try {
      const result = await setConnectionMasking(projectId, connectionId, next);
      if (!isCurrent()) return;
      if (!result.success) throw new Error(result.error);
      refreshSidebar();
      if (onChanged) {
        toast.success(message);
        onChanged();
      } else {
        toast.success(t('grid.masking.appliedNextRun'));
      }
    } catch (error) {
      if (!isCurrent()) return;
      toast.error(error instanceof Error && error.message ? error.message : t('common.error'));
    } finally {
      busy.current = false;
    }
  }

  function remove(column: string, isCurrent = captureContext()) {
    const rule = findRule(masking, tableName, column);
    if (rule) void save(withoutRule(masking, rule), t('grid.masking.removed'), isCurrent);
  }

  function toggle(column: string) {
    // Missing metadata is not an empty policy that can safely be overwritten.
    if (!savedConnection) return;
    if (findRule(masking, tableName, column)) {
      if (environment === 'production') {
        const isCurrent = captureContext();
        setPendingRemoval(() => () => remove(column, isCurrent));
      } else {
        remove(column);
      }
      return;
    }
    void save(
      withRule(masking, { table: tableName ?? '', column, mode: 'hidden' }),
      t('grid.masking.applied')
    );
  }

  const columnMask: ColumnMaskMenu | undefined = connectionId
    ? {
        hasRule: column => Boolean(findRule(masking, tableName, column)),
        isMasked: column => maskedColumns.has(column),
        canAdd: Boolean(savedConnection) && isFeatureEnabled('column_masking'),
        toggle,
      }
    : undefined;

  const removalDialog = (
    <ProductionConfirmDialog
      open={pendingRemoval !== null}
      title={t('connection.masking.removeConfirmTitle')}
      description={t('connection.masking.removeConfirmDescription')}
      confirmationLabel={confirmationLabel}
      confirmLabel={t('connection.masking.removeConfirmLabel')}
      onConfirm={() => {
        pendingRemoval?.();
        setPendingRemoval(null);
      }}
      onOpenChange={open => {
        if (!open) setPendingRemoval(null);
      }}
    />
  );

  return { columnMask, removalDialog };
}

// SPDX-License-Identifier: BUSL-1.1

import { useState } from 'react';
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
  const [pendingRemoval, setPendingRemoval] = useState<string | null>(null);

  const masking =
    savedConnections.find(connection => connection.id === connectionId)?.masking ?? EMPTY_MASKING;

  async function save(next: ConnectionMasking, message: string) {
    if (!connectionId) return;
    try {
      const result = await setConnectionMasking(projectId, connectionId, next);
      if (!result.success) throw new Error(result.error);
      refreshSidebar();
      if (onChanged) {
        toast.success(message);
        onChanged();
      } else {
        toast.success(t('grid.masking.appliedNextRun'));
      }
    } catch (error) {
      toast.error(error instanceof Error && error.message ? error.message : t('common.error'));
    }
  }

  function remove(column: string) {
    const rule = findRule(masking, tableName, column);
    if (rule) void save(withoutRule(masking, rule), t('grid.masking.removed'));
  }

  function toggle(column: string) {
    if (findRule(masking, tableName, column)) {
      if (environment === 'production') {
        setPendingRemoval(column);
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
        canAdd: isFeatureEnabled('column_masking'),
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
        if (pendingRemoval) remove(pendingRemoval);
        setPendingRemoval(null);
      }}
      onOpenChange={open => {
        if (!open) setPendingRemoval(null);
      }}
    />
  );

  return { columnMask, removalDialog };
}

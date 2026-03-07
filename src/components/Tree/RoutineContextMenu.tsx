// SPDX-License-Identifier: Apache-2.0

import { Code2, Trash2 } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { DangerConfirmDialog } from '@/components/Guard/DangerConfirmDialog';

import {
  ContextMenu,
  ContextMenuContent,
  ContextMenuItem,
  ContextMenuSeparator,
  ContextMenuTrigger,
} from '@/components/ui/context-menu';
import { notify } from '../../lib/notify';
import { dropRoutine, type Environment, type Routine } from '../../lib/tauri';

interface RoutineContextMenuProps {
  routine: Routine;
  sessionId: string;
  environment: Environment;
  readOnly: boolean;
  onViewSource: (routine: Routine) => void;
  onDrop: () => void;
  children: React.ReactNode;
}

/**
 * Right-click context menu wrapper for routine items (functions/procedures).
 */
export function RoutineContextMenu({
  routine,
  sessionId,
  environment,
  readOnly,
  onViewSource,
  onDrop,
  children,
}: RoutineContextMenuProps) {
  const { t } = useTranslation();
  const [showDropConfirm, setShowDropConfirm] = useState(false);
  const [loading, setLoading] = useState(false);

  const isProduction = environment === 'production';
  const typeLabel =
    routine.routine_type === 'Function'
      ? t('routineManager.function')
      : t('routineManager.procedure');
  const confirmationLabel = isProduction ? routine.name : undefined;

  async function handleDrop() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      const result = await dropRoutine(
        sessionId,
        routine.namespace.database,
        routine.namespace.schema,
        routine.name,
        routine.routine_type,
        routine.arguments || undefined,
        true
      );

      if (result.success) {
        notify.success(t('routineManager.dropSuccess', { type: typeLabel, name: routine.name }));
        setShowDropConfirm(false);
        onDrop();
      } else {
        notify.error(
          t('routineManager.dropFailed', { type: typeLabel, name: routine.name }),
          result.error
        );
      }
    } catch (err) {
      notify.error(t('common.error'), err);
    } finally {
      setLoading(false);
    }
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => onViewSource(routine)}>
            <Code2 size={14} className="mr-2" />
            {t('routineManager.viewSource')}
          </ContextMenuItem>

          <ContextMenuSeparator />

          <ContextMenuItem
            onClick={() => setShowDropConfirm(true)}
            disabled={readOnly}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 size={14} className="mr-2" />
            {t('routineManager.drop')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DangerConfirmDialog
        open={showDropConfirm}
        onOpenChange={open => !open && setShowDropConfirm(false)}
        title={t('routineManager.dropTitle')}
        description={t('routineManager.dropConfirm', {
          type: typeLabel,
          name: routine.name,
        })}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.delete')}
        loading={loading}
        onConfirm={handleDrop}
      />
    </>
  );
}

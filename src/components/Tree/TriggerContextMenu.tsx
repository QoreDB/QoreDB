// SPDX-License-Identifier: Apache-2.0

import { Code2, Power, PowerOff, Trash2 } from 'lucide-react';
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
import {
  dropTrigger,
  toggleTrigger,
  type Environment,
  type Trigger,
} from '../../lib/tauri';

interface TriggerContextMenuProps {
  trigger: Trigger;
  sessionId: string;
  environment: Environment;
  readOnly: boolean;
  supportsToggle: boolean;
  onViewSource: (trigger: Trigger) => void;
  onDrop: () => void;
  onToggle: () => void;
  children: React.ReactNode;
}

/**
 * Right-click context menu wrapper for trigger items.
 */
export function TriggerContextMenu({
  trigger,
  sessionId,
  environment,
  readOnly,
  supportsToggle,
  onViewSource,
  onDrop,
  onToggle,
  children,
}: TriggerContextMenuProps) {
  const { t } = useTranslation();
  const [showDropConfirm, setShowDropConfirm] = useState(false);
  const [loading, setLoading] = useState(false);

  const isProduction = environment === 'production';
  const confirmationLabel = isProduction ? trigger.name : undefined;

  async function handleDrop() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      const result = await dropTrigger(
        sessionId,
        trigger.namespace.database,
        trigger.namespace.schema,
        trigger.name,
        trigger.table_name,
        true
      );

      if (result.success) {
        notify.success(
          t('triggerManager.dropSuccess', { name: trigger.name })
        );
        setShowDropConfirm(false);
        onDrop();
      } else {
        notify.error(
          t('triggerManager.dropFailed', { name: trigger.name }),
          result.error
        );
      }
    } catch (err) {
      notify.error(t('common.error'), err);
    } finally {
      setLoading(false);
    }
  }

  async function handleToggle() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    try {
      const result = await toggleTrigger(
        sessionId,
        trigger.namespace.database,
        trigger.namespace.schema,
        trigger.name,
        trigger.table_name,
        !trigger.enabled
      );

      if (result.success) {
        const key = trigger.enabled
          ? 'triggerManager.disableSuccess'
          : 'triggerManager.enableSuccess';
        notify.success(t(key, { name: trigger.name }));
        onToggle();
      } else {
        notify.error(
          t('triggerManager.toggleFailed', { name: trigger.name }),
          result.error
        );
      }
    } catch (err) {
      notify.error(t('common.error'), err);
    }
  }

  return (
    <>
      <ContextMenu>
        <ContextMenuTrigger asChild>{children}</ContextMenuTrigger>
        <ContextMenuContent className="w-48">
          <ContextMenuItem onClick={() => onViewSource(trigger)}>
            <Code2 size={14} className="mr-2" />
            {t('triggerManager.viewSource')}
          </ContextMenuItem>

          {supportsToggle && (
            <ContextMenuItem onClick={handleToggle} disabled={readOnly}>
              {trigger.enabled ? (
                <PowerOff size={14} className="mr-2" />
              ) : (
                <Power size={14} className="mr-2" />
              )}
              {trigger.enabled
                ? t('triggerManager.disable')
                : t('triggerManager.enable')}
            </ContextMenuItem>
          )}

          <ContextMenuSeparator />

          <ContextMenuItem
            onClick={() => setShowDropConfirm(true)}
            disabled={readOnly}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 size={14} className="mr-2" />
            {t('triggerManager.drop')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DangerConfirmDialog
        open={showDropConfirm}
        onOpenChange={open => !open && setShowDropConfirm(false)}
        title={t('triggerManager.dropTitle')}
        description={t('triggerManager.dropConfirm', {
          name: trigger.name,
          table: trigger.table_name,
        })}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.delete')}
        loading={loading}
        onConfirm={handleDrop}
      />
    </>
  );
}

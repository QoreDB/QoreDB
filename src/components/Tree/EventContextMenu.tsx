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
import { dropEvent, type DatabaseEvent, type Environment } from '../../lib/tauri';

interface EventContextMenuProps {
  event: DatabaseEvent;
  sessionId: string;
  environment: Environment;
  readOnly: boolean;
  onViewSource: (event: DatabaseEvent) => void;
  onDrop: () => void;
  children: React.ReactNode;
}

/**
 * Right-click context menu wrapper for MySQL scheduled event items.
 */
export function EventContextMenu({
  event,
  sessionId,
  environment,
  readOnly,
  onViewSource,
  onDrop,
  children,
}: EventContextMenuProps) {
  const { t } = useTranslation();
  const [showDropConfirm, setShowDropConfirm] = useState(false);
  const [loading, setLoading] = useState(false);

  const isProduction = environment === 'production';
  const confirmationLabel = isProduction ? event.name : undefined;

  async function handleDrop() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      const result = await dropEvent(
        sessionId,
        event.namespace.database,
        event.namespace.schema,
        event.name,
        true
      );

      if (result.success) {
        notify.success(t('eventManager.dropSuccess', { name: event.name }));
        setShowDropConfirm(false);
        onDrop();
      } else {
        notify.error(t('eventManager.dropFailed', { name: event.name }), result.error);
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
          <ContextMenuItem onClick={() => onViewSource(event)}>
            <Code2 size={14} className="mr-2" />
            {t('eventManager.viewSource')}
          </ContextMenuItem>

          <ContextMenuSeparator />

          <ContextMenuItem
            onClick={() => setShowDropConfirm(true)}
            disabled={readOnly}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 size={14} className="mr-2" />
            {t('eventManager.drop')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DangerConfirmDialog
        open={showDropConfirm}
        onOpenChange={open => !open && setShowDropConfirm(false)}
        title={t('eventManager.dropTitle')}
        description={t('eventManager.dropConfirm', {
          name: event.name,
        })}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.delete')}
        loading={loading}
        onConfirm={handleDrop}
      />
    </>
  );
}

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
import { dropSequence, type Environment, type Sequence } from '../../lib/tauri';

interface SequenceContextMenuProps {
  sequence: Sequence;
  sessionId: string;
  environment: Environment;
  readOnly: boolean;
  onViewSource: (sequence: Sequence) => void;
  onDrop: () => void;
  children: React.ReactNode;
}

/**
 * Right-click context menu wrapper for MariaDB sequence items.
 */
export function SequenceContextMenu({
  sequence,
  sessionId,
  environment,
  readOnly,
  onViewSource,
  onDrop,
  children,
}: SequenceContextMenuProps) {
  const { t } = useTranslation();
  const [showDropConfirm, setShowDropConfirm] = useState(false);
  const [loading, setLoading] = useState(false);

  const isProduction = environment === 'production';
  const confirmationLabel = isProduction ? sequence.name : undefined;

  async function handleDrop() {
    if (readOnly) {
      notify.error(t('environment.blocked'));
      return;
    }
    setLoading(true);
    try {
      const result = await dropSequence(
        sessionId,
        sequence.namespace.database,
        sequence.namespace.schema,
        sequence.name,
        true
      );

      if (result.success) {
        notify.success(t('sequenceManager.dropSuccess', { name: sequence.name }));
        setShowDropConfirm(false);
        onDrop();
      } else {
        notify.error(t('sequenceManager.dropFailed', { name: sequence.name }), result.error);
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
          <ContextMenuItem onClick={() => onViewSource(sequence)}>
            <Code2 size={14} className="mr-2" />
            {t('sequenceManager.viewSource')}
          </ContextMenuItem>

          <ContextMenuSeparator />

          <ContextMenuItem
            onClick={() => setShowDropConfirm(true)}
            disabled={readOnly}
            className="text-destructive focus:text-destructive"
          >
            <Trash2 size={14} className="mr-2" />
            {t('sequenceManager.drop')}
          </ContextMenuItem>
        </ContextMenuContent>
      </ContextMenu>

      <DangerConfirmDialog
        open={showDropConfirm}
        onOpenChange={open => !open && setShowDropConfirm(false)}
        title={t('sequenceManager.dropTitle')}
        description={t('sequenceManager.dropConfirm', {
          name: sequence.name,
        })}
        confirmationLabel={confirmationLabel}
        confirmLabel={t('common.delete')}
        loading={loading}
        onConfirm={handleDrop}
      />
    </>
  );
}

// SPDX-License-Identifier: Apache-2.0

import { useCallback, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { emitUiEvent, UI_EVENT_CONNECTIONS_CHANGED } from '@/lib/uiEvents';
import {
  deleteSavedConnection,
  duplicateSavedConnection,
  getConnectionCredentials,
  type SavedConnection,
  testSavedConnection,
} from '../../lib/tauri';

interface UseConnectionActionsOptions {
  connection: SavedConnection;
  onEdit: (connection: SavedConnection, password: string) => void;
  onDeleted: () => void;
  onAfterAction?: () => void;
}

export function useConnectionActions({
  connection,
  onEdit,
  onDeleted,
  onAfterAction,
}: UseConnectionActionsOptions) {
  const [testing, setTesting] = useState(false);
  const [deleting, setDeleting] = useState(false);
  const [duplicating, setDuplicating] = useState(false);
  const { t } = useTranslation();

  const handleTest = useCallback(async () => {
    setTesting(true);
    try {
      const result = await testSavedConnection('default', connection.id);

      if (result.success) {
        toast.success(t('connection.menu.testTitleSuccess', { name: connection.name }), {
          description: `${connection.host}:${connection.port}`,
        });
      } else {
        toast.error(t('connection.testFail'), {
          description: result.error || t('common.unknownError'),
        });
      }
    } catch (err) {
      toast.error(t('connection.testFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      setTesting(false);
      onAfterAction?.();
    }
  }, [connection, onAfterAction, t]);

  const handleEdit = useCallback(async () => {
    try {
      const credsResult = await getConnectionCredentials('default', connection.id);

      // Allow empty password (e.g. for MongoDB)
      if (
        !credsResult.success ||
        credsResult.password === undefined ||
        credsResult.password === null
      ) {
        toast.error(t('connection.failedRetrieveCredentialsEdit'));
        return;
      }

      onEdit(connection, credsResult.password);
      onAfterAction?.();
    } catch {
      toast.error(t('connection.menu.credentialLoadFail'));
    }
  }, [connection, onAfterAction, onEdit, t]);

  const handleDelete = useCallback(async () => {
    setDeleting(true);
    try {
      const result = await deleteSavedConnection('default', connection.id);
      if (result.success) {
        toast.success(t('connection.menu.deletedSuccess', { name: connection.name }));
        emitUiEvent(UI_EVENT_CONNECTIONS_CHANGED);
        onDeleted();
      } else {
        toast.error(t('connection.menu.deleteFail'), {
          description: result.error,
        });
      }
    } catch (err) {
      toast.error(t('connection.menu.deleteFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      setDeleting(false);
      onAfterAction?.();
    }
  }, [connection, onAfterAction, onDeleted, t]);

  const handleDuplicate = useCallback(async () => {
    setDuplicating(true);
    try {
      const result = await duplicateSavedConnection('default', connection.id);
      if (result.success && result.connection) {
        toast.success(t('connection.menu.duplicateSuccess', { name: result.connection.name }));
        emitUiEvent(UI_EVENT_CONNECTIONS_CHANGED);
        onDeleted();
      } else {
        toast.error(t('connection.menu.duplicateFail'), {
          description: result.error || t('common.unknownError'),
        });
      }
    } catch (err) {
      toast.error(t('connection.menu.duplicateFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      setDuplicating(false);
      onAfterAction?.();
    }
  }, [connection.id, onAfterAction, onDeleted, t]);

  return {
    testing,
    deleting,
    duplicating,
    handleTest,
    handleEdit,
    handleDelete,
    handleDuplicate,
  };
}

import { useCallback, useState } from 'react';
import {
  SavedConnection,
  deleteSavedConnection,
  testSavedConnection,
  getConnectionCredentials,
} from '../../lib/tauri';
import { toast } from 'sonner';
import { useTranslation } from 'react-i18next';

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
      if (!credsResult.success || !credsResult.password) {
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
    if (!confirm(t('connection.menu.deleteConfirm', { name: connection.name }))) {
      return;
    }

    setDeleting(true);
    try {
      const result = await deleteSavedConnection('default', connection.id);
      if (result.success) {
        toast.success(t('connection.menu.deletedSuccess', { name: connection.name }));
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

  const handleDuplicate = useCallback(() => {
    toast.info(t('connection.menu.duplicateComingSoon'));
    onAfterAction?.();
  }, [onAfterAction, t]);

  return {
    testing,
    deleting,
    handleTest,
    handleEdit,
    handleDelete,
    handleDuplicate,
  };
}

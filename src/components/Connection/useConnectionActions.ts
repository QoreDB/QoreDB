// SPDX-License-Identifier: Apache-2.0

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { emitUiEvent, UI_EVENT_CONNECTIONS_CHANGED } from '@/lib/events/uiEvents';
import { captureWorkspaceScope, useWorkspaceStore } from '@/lib/stores/workspaceStore';
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
  const workspace = useWorkspaceStore(state => state);
  const { projectId } = workspace;
  const inWorkspace = useMemo(() => captureWorkspaceScope(workspace.projectId), [workspace]);
  const mounted = useRef(true);
  const currentConnection = useRef(connection.id);
  currentConnection.current = connection.id;
  const busy = useRef(false);
  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
    };
  }, []);
  const isCurrent = useCallback(
    () => mounted.current && inWorkspace() && currentConnection.current === connection.id,
    [inWorkspace, connection.id]
  );

  const handleTest = useCallback(async () => {
    if (!isCurrent() || busy.current) return;
    busy.current = true;
    setTesting(true);
    try {
      const result = await testSavedConnection(projectId, connection.id);

      if (!isCurrent()) return;
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
      if (!isCurrent()) return;
      toast.error(t('connection.testFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      busy.current = false;
      if (mounted.current) setTesting(false);
      if (isCurrent()) onAfterAction?.();
    }
  }, [connection, onAfterAction, t, projectId, isCurrent]);

  const handleEdit = useCallback(async () => {
    if (!isCurrent() || busy.current) return;
    busy.current = true;
    try {
      const credsResult = await getConnectionCredentials(projectId, connection.id);

      if (!isCurrent()) return;
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
      if (!isCurrent()) return;
      toast.error(t('connection.menu.credentialLoadFail'));
    } finally {
      busy.current = false;
    }
  }, [connection, onAfterAction, onEdit, t, projectId, isCurrent]);

  const handleDelete = useCallback(async () => {
    if (!isCurrent() || busy.current) return;
    busy.current = true;
    setDeleting(true);
    try {
      const result = await deleteSavedConnection(projectId, connection.id);
      if (!isCurrent()) return;
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
      if (!isCurrent()) return;
      toast.error(t('connection.menu.deleteFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      busy.current = false;
      if (mounted.current) setDeleting(false);
      if (isCurrent()) onAfterAction?.();
    }
  }, [connection, onAfterAction, onDeleted, t, projectId, isCurrent]);

  const handleDuplicate = useCallback(async () => {
    if (!isCurrent() || busy.current) return;
    busy.current = true;
    setDuplicating(true);
    try {
      const result = await duplicateSavedConnection(projectId, connection.id);
      if (!isCurrent()) return;
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
      if (!isCurrent()) return;
      toast.error(t('connection.menu.duplicateFail'), {
        description: err instanceof Error ? err.message : t('common.unknownError'),
      });
    } finally {
      busy.current = false;
      if (mounted.current) setDuplicating(false);
      if (isCurrent()) onAfterAction?.();
    }
  }, [connection.id, onAfterAction, onDeleted, t, projectId, isCurrent]);

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

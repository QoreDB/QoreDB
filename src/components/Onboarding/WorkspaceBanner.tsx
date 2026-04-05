// SPDX-License-Identifier: Apache-2.0

import { FolderOpen, X } from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { CreateWorkspaceDialog } from '@/components/Workspace/CreateWorkspaceDialog';
import { listSavedConnections } from '@/lib/tauri';
import { useWorkspace } from '@/providers/WorkspaceProvider';

const DISMISS_KEY = 'qoredb_workspace_banner_dismissed';
const MIN_CONNECTIONS = 4;

export function WorkspaceBanner() {
  const { t } = useTranslation();
  const { activeWorkspace, projectId } = useWorkspace();
  const [visible, setVisible] = useState(false);
  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const isDefault = !activeWorkspace || activeWorkspace.source === 'default';

  useEffect(() => {
    if (!isDefault) {
      setVisible(false);
      return;
    }

    if (localStorage.getItem(DISMISS_KEY) === 'true') {
      return;
    }

    let cancelled = false;
    listSavedConnections(projectId)
      .then(saved => {
        if (!cancelled && saved.length >= MIN_CONNECTIONS) {
          setVisible(true);
        }
      })
      .catch(() => {});

    return () => {
      cancelled = true;
    };
  }, [isDefault, projectId]);

  function handleDismiss() {
    localStorage.setItem(DISMISS_KEY, 'true');
    setVisible(false);
  }

  if (!visible) return null;

  return (
    <>
      <div className="mx-auto max-w-md w-full rounded-lg border border-border bg-muted/30 p-4 mb-4">
        <div className="flex items-start gap-3">
          <div className="p-2 rounded-md bg-emerald-500/10 shrink-0">
            <FolderOpen size={18} className="text-emerald-500" />
          </div>
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium text-foreground">{t('onboarding.workspace.title')}</p>
            <p className="text-xs text-muted-foreground mt-1">
              {t('onboarding.workspace.description')}
            </p>
            <Button
              variant="outline"
              size="sm"
              className="mt-3"
              onClick={() => setCreateDialogOpen(true)}
            >
              {t('workspace.create')}
            </Button>
          </div>
          <button
            type="button"
            onClick={handleDismiss}
            className="text-muted-foreground hover:text-foreground transition-colors shrink-0"
          >
            <X size={14} />
          </button>
        </div>
      </div>

      <CreateWorkspaceDialog open={createDialogOpen} onOpenChange={setCreateDialogOpen} />
    </>
  );
}

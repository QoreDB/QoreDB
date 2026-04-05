// SPDX-License-Identifier: Apache-2.0

import { ChevronDown, FolderOpen, LogOut, Pencil, Plus } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { open as openDialog } from '@tauri-apps/plugin-dialog';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { renameWorkspace } from '@/lib/tauri';
import { CreateWorkspaceDialog } from '@/components/Workspace/CreateWorkspaceDialog';

export function WorkspaceSwitcher() {
  const { t } = useTranslation();
  const { activeWorkspace, recentWorkspaces, switchWorkspace, switchToDefault, openWorkspace } =
    useWorkspace();

  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const isDefault = !activeWorkspace || activeWorkspace.source === 'default';

  const displayName = isDefault ? t('workspace.default') : activeWorkspace?.manifest.name;
  const badge = activeWorkspace?.source === 'detected' ? 'CWD' : null;

  async function handleOpen() {
    const selected = await openDialog({
      directory: true,
      title: t('workspace.open'),
    });
    if (!selected) return;

    const qoredbPath = selected.endsWith('.qoredb') ? selected : `${selected}/.qoredb`;
    await openWorkspace(qoredbPath);
  }

  async function handleRename() {
    const newName = window.prompt(
      t('workspace.renamePrompt'),
      activeWorkspace?.manifest.name ?? ''
    );
    if (!newName?.trim()) return;

    const result = await renameWorkspace(newName.trim());
    if (result.success && result.workspace) {
      // Refresh active workspace state
      await switchWorkspace(result.workspace.path);
    } else if (result.error) {
      toast.error(result.error);
    }
  }

  return (
    <>
      <div className="px-3 py-1.5 border-b border-border" data-tour="workspace-switcher">
        <DropdownMenu>
          <DropdownMenuTrigger asChild>
            <button
              type="button"
              className="flex items-center gap-2 w-full px-2 py-1 text-xs rounded-md hover:bg-muted transition-colors text-left"
            >
              <span className="relative shrink-0">
                <FolderOpen
                  size={13}
                  className={isDefault ? 'text-muted-foreground' : 'text-emerald-500'}
                />
                {!isDefault && (
                  <span className="absolute -top-0.5 -right-0.5 h-1.5 w-1.5 rounded-full bg-emerald-500" />
                )}
              </span>
              <span className="truncate flex-1 font-medium">{displayName}</span>
              {badge && (
                <span className="text-[10px] px-1 py-0.5 rounded bg-accent/20 text-accent-foreground font-mono shrink-0">
                  {badge}
                </span>
              )}
              <ChevronDown size={12} className="text-muted-foreground shrink-0" />
            </button>
          </DropdownMenuTrigger>
          <DropdownMenuContent align="start" className="w-64">
            {/* Active workspace actions */}
            {!isDefault && (
              <>
                <DropdownMenuItem onClick={handleRename}>
                  <Pencil size={14} className="mr-2" />
                  {t('workspace.rename')}
                </DropdownMenuItem>
                <DropdownMenuItem onClick={() => switchToDefault()}>
                  <LogOut size={14} className="mr-2" />
                  {t('workspace.leave')}
                </DropdownMenuItem>
                <DropdownMenuSeparator />
              </>
            )}

            {/* Recent workspaces */}
            {recentWorkspaces.length > 0 && (
              <>
                <DropdownMenuLabel className="text-[11px]">
                  {t('workspace.recent')}
                </DropdownMenuLabel>
                {recentWorkspaces
                  .filter(r => r.path !== activeWorkspace?.path)
                  .slice(0, 5)
                  .map(recent => (
                    <DropdownMenuItem
                      key={recent.path}
                      onClick={() => switchWorkspace(recent.path)}
                    >
                      <FolderOpen size={14} className="mr-2 shrink-0" />
                      <span className="truncate">{recent.name}</span>
                    </DropdownMenuItem>
                  ))}
                <DropdownMenuSeparator />
              </>
            )}

            {/* Global actions */}
            <DropdownMenuItem onClick={handleOpen}>
              <FolderOpen size={14} className="mr-2" />
              {t('workspace.open')}
            </DropdownMenuItem>
            <DropdownMenuItem onClick={() => setCreateDialogOpen(true)}>
              <Plus size={14} className="mr-2" />
              {t('workspace.create')}
            </DropdownMenuItem>
          </DropdownMenuContent>
        </DropdownMenu>
      </div>

      <CreateWorkspaceDialog open={createDialogOpen} onOpenChange={setCreateDialogOpen} />
    </>
  );
}

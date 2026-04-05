// SPDX-License-Identifier: Apache-2.0

import { FolderOpen, GitBranch, Map, Users } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { CreateWorkspaceDialog } from '@/components/Workspace/CreateWorkspaceDialog';
import { useTourManager } from '@/hooks/useTourManager';
import { useWorkspace } from '@/providers/WorkspaceProvider';
import { SettingsCard } from '../SettingsCard';

interface WorkspacesSectionProps {
  searchQuery?: string;
}

export function WorkspacesSection({ searchQuery }: WorkspacesSectionProps) {
  const { t } = useTranslation();
  const { activeWorkspace } = useWorkspace();
  const tourManager = useTourManager();
  const [createDialogOpen, setCreateDialogOpen] = useState(false);

  const isDefault = !activeWorkspace || activeWorkspace.source === 'default';

  return (
    <div className="space-y-1 divide-y divide-border/50">
      <SettingsCard
        title={t('settings.workspaces.whatTitle')}
        description={t('settings.workspaces.whatDescription')}
        searchQuery={searchQuery}
      >
        <div className="grid grid-cols-1 sm:grid-cols-3 gap-3 mt-2">
          <div className="flex items-start gap-2.5 p-3 rounded-lg bg-muted/40 border border-border/50">
            <GitBranch size={16} className="text-emerald-500 mt-0.5 shrink-0" />
            <div>
              <p className="text-xs font-medium">{t('settings.workspaces.benefitGit')}</p>
              <p className="text-[11px] text-muted-foreground mt-0.5">
                {t('settings.workspaces.benefitGitDesc')}
              </p>
            </div>
          </div>
          <div className="flex items-start gap-2.5 p-3 rounded-lg bg-muted/40 border border-border/50">
            <Users size={16} className="text-blue-500 mt-0.5 shrink-0" />
            <div>
              <p className="text-xs font-medium">{t('settings.workspaces.benefitTeam')}</p>
              <p className="text-[11px] text-muted-foreground mt-0.5">
                {t('settings.workspaces.benefitTeamDesc')}
              </p>
            </div>
          </div>
          <div className="flex items-start gap-2.5 p-3 rounded-lg bg-muted/40 border border-border/50">
            <FolderOpen size={16} className="text-amber-500 mt-0.5 shrink-0" />
            <div>
              <p className="text-xs font-medium">{t('settings.workspaces.benefitOrganize')}</p>
              <p className="text-[11px] text-muted-foreground mt-0.5">
                {t('settings.workspaces.benefitOrganizeDesc')}
              </p>
            </div>
          </div>
        </div>
      </SettingsCard>

      <SettingsCard
        title={t('settings.workspaces.activeTitle')}
        description={t('settings.workspaces.activeDescription')}
        searchQuery={searchQuery}
      >
        <div className="flex items-center gap-3 p-3 rounded-lg bg-muted/30 border border-border/50">
          <FolderOpen
            size={18}
            className={isDefault ? 'text-muted-foreground' : 'text-emerald-500'}
          />
          <div className="flex-1 min-w-0">
            <p className="text-sm font-medium truncate">
              {isDefault ? t('workspace.default') : activeWorkspace?.manifest.name}
            </p>
            {!isDefault && (
              <p className="text-[11px] text-muted-foreground truncate">{activeWorkspace?.path}</p>
            )}
          </div>
          {activeWorkspace?.source === 'detected' && (
            <span className="text-[10px] px-1.5 py-0.5 rounded bg-accent/20 text-accent-foreground font-mono shrink-0">
              CWD
            </span>
          )}
        </div>
        <div className="flex gap-2 mt-3">
          {isDefault && (
            <Button variant="outline" size="sm" onClick={() => setCreateDialogOpen(true)}>
              {t('workspace.create')}
            </Button>
          )}
        </div>
      </SettingsCard>

      <SettingsCard
        title={t('settings.workspaces.quickstartTitle')}
        description={t('settings.workspaces.quickstartDescription')}
        searchQuery={searchQuery}
      >
        <Button variant="outline" size="sm" onClick={() => tourManager.startTour('workspaces')}>
          <Map size={14} className="mr-1.5" />
          {t('settings.workspaces.startTour')}
        </Button>
      </SettingsCard>

      <CreateWorkspaceDialog open={createDialogOpen} onOpenChange={setCreateDialogOpen} />
    </div>
  );
}

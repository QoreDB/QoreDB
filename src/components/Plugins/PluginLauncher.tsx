// SPDX-License-Identifier: Apache-2.0

import { Check, Puzzle, Settings2, Zap } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuSub,
  DropdownMenuSubContent,
  DropdownMenuSubTrigger,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip } from '@/components/ui/tooltip';
import { splitContributionId } from '@/lib/plugins';
import { openSettingsSection } from '@/lib/stores/modalStore';
import { usePlugins } from '@/providers/PluginProvider';

interface PluginLauncherProps {
  /** Runs a contributed command (namespaced id) — opens the output tab and
   *  records the run. Provided by the host so the launcher stays decoupled
   *  from tab management. */
  onRunCommand: (namespacedId: string) => void;
}

/**
 * Always-visible home for installed plugins. The menu is organised by plugin:
 * each active plugin owns a submenu that surfaces whatever it contributes —
 * commands to run, themes to apply. This stays generic, so a plugin is never
 * special-cased by what it happens to contribute; one that works silently
 * through query hooks still shows up (and links to its settings).
 */
export function PluginLauncher({ onRunCommand }: PluginLauncherProps) {
  const { t } = useTranslation();
  const { plugins, contributions, activeThemeId, setActiveTheme } = usePlugins();

  // Each active plugin paired with the contributions it owns. Contribution ids
  // are namespaced `<plugin-id>::<local-id>`, so origin is recovered by split.
  const entries = useMemo(() => {
    const active = plugins.filter(p => p.enabled && p.compatible);
    return active.map(plugin => ({
      plugin,
      commands: contributions.commands.filter(
        c => splitContributionId(c.id).pluginId === plugin.manifest.id
      ),
      themes: contributions.themes.filter(
        th => splitContributionId(th.id).pluginId === plugin.manifest.id
      ),
    }));
  }, [plugins, contributions.commands, contributions.themes]);

  if (entries.length === 0) return null;

  return (
    <DropdownMenu>
      <Tooltip content={t('pluginLauncher.tooltip')}>
        <DropdownMenuTrigger asChild>
          <Button
            variant="ghost"
            size="icon"
            className="h-7 w-7 text-muted-foreground hover:text-foreground transition-transform duration-200 active:scale-90"
            aria-label={t('pluginLauncher.tooltip')}
          >
            <Puzzle className="w-4 h-4" />
          </Button>
        </DropdownMenuTrigger>
      </Tooltip>
      <DropdownMenuContent align="end" className="w-60">
        <DropdownMenuLabel className="text-muted-foreground">
          {t('pluginLauncher.active')}
        </DropdownMenuLabel>

        {entries.map(({ plugin, commands, themes }) => {
          const runtimeBadge = plugin.manifest.runtime && (
            <Tooltip content={t('pluginLauncher.runsInBackground')}>
              <Zap size={12} className="shrink-0 text-accent" />
            </Tooltip>
          );

          // A plugin with nothing to surface still appears, linking to its
          // settings — it isn't hidden just because it has no menu actions.
          if (commands.length === 0 && themes.length === 0) {
            return (
              <DropdownMenuItem
                key={plugin.manifest.id}
                onClick={() => openSettingsSection('plugins')}
                className="gap-2"
              >
                <span className="flex-1 truncate">{plugin.manifest.name}</span>
                {runtimeBadge}
              </DropdownMenuItem>
            );
          }

          return (
            <DropdownMenuSub key={plugin.manifest.id}>
              <DropdownMenuSubTrigger className="gap-2">
                <span className="flex-1 truncate">{plugin.manifest.name}</span>
                {runtimeBadge}
              </DropdownMenuSubTrigger>
              <DropdownMenuSubContent className="w-56">
                {commands.map(cmd => (
                  <DropdownMenuItem key={cmd.id} onClick={() => onRunCommand(cmd.id)}>
                    <span className="truncate">{cmd.label}</span>
                  </DropdownMenuItem>
                ))}

                {themes.length > 0 && (
                  <>
                    {commands.length > 0 && <DropdownMenuSeparator />}
                    <DropdownMenuItem onClick={() => setActiveTheme(null)} className="gap-2">
                      <span className="flex-1 truncate">{t('pluginLauncher.themeDefault')}</span>
                      {activeThemeId === null && <Check size={14} className="text-accent" />}
                    </DropdownMenuItem>
                    {themes.map(theme => (
                      <DropdownMenuItem
                        key={theme.id}
                        onClick={() => setActiveTheme(activeThemeId === theme.id ? null : theme.id)}
                        className="gap-2"
                      >
                        <span className="flex-1 truncate">{theme.name}</span>
                        {activeThemeId === theme.id && <Check size={14} className="text-accent" />}
                      </DropdownMenuItem>
                    ))}
                  </>
                )}
              </DropdownMenuSubContent>
            </DropdownMenuSub>
          );
        })}

        <DropdownMenuSeparator />
        <DropdownMenuItem onClick={() => openSettingsSection('plugins')} className="gap-2">
          <Settings2 size={14} />
          {t('pluginLauncher.manage')}
        </DropdownMenuItem>
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

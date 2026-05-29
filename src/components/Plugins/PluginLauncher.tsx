// SPDX-License-Identifier: Apache-2.0

import { Check, Palette, Puzzle, Terminal } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import {
  DropdownMenu,
  DropdownMenuContent,
  DropdownMenuGroup,
  DropdownMenuItem,
  DropdownMenuLabel,
  DropdownMenuSeparator,
  DropdownMenuTrigger,
} from '@/components/ui/dropdown-menu';
import { Tooltip } from '@/components/ui/tooltip';
import { splitContributionId } from '@/lib/plugins';
import { usePlugins } from '@/providers/PluginProvider';

interface PluginLauncherProps {
  /** Runs a contributed command (namespaced id) — opens the output tab and
   *  records the run. Provided by the host so the launcher stays decoupled
   *  from tab management. */
  onRunCommand: (namespacedId: string) => void;
}

/**
 * Always-visible entry point to enabled plugins: run their commands and
 * switch plugin themes from a single titlebar menu. Hidden when no plugin
 * contributes a command or a theme, so it never adds noise for users
 * without plugins.
 */
export function PluginLauncher({ onRunCommand }: PluginLauncherProps) {
  const { t } = useTranslation();
  const { plugins, contributions, activeThemeId, setActiveTheme } = usePlugins();

  const pluginName = useMemo(() => {
    const byId = new Map(plugins.map(p => [p.manifest.id, p.manifest.name]));
    return (namespacedId: string) => {
      const { pluginId } = splitContributionId(namespacedId);
      return byId.get(pluginId) ?? pluginId;
    };
  }, [plugins]);

  // Group commands by their owning plugin so each plugin gets its own
  // labelled section in the menu.
  const commandGroups = useMemo(() => {
    const groups = new Map<
      string,
      { id: string; name: string; commands: typeof contributions.commands }
    >();
    for (const cmd of contributions.commands) {
      const { pluginId } = splitContributionId(cmd.id);
      const group = groups.get(pluginId);
      if (group) {
        group.commands.push(cmd);
      } else {
        groups.set(pluginId, { id: pluginId, name: pluginName(cmd.id), commands: [cmd] });
      }
    }
    return [...groups.values()];
  }, [contributions.commands, pluginName]);

  const { commands, themes } = contributions;
  if (commands.length === 0 && themes.length === 0) return null;

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
      <DropdownMenuContent align="end" className="w-64">
        {commandGroups.map((group, index) => (
          <DropdownMenuGroup key={group.id}>
            {index > 0 && <DropdownMenuSeparator />}
            <DropdownMenuLabel className="flex items-center gap-1.5 text-muted-foreground">
              <Terminal size={12} />
              {group.name}
            </DropdownMenuLabel>
            {group.commands.map(cmd => (
              <DropdownMenuItem key={cmd.id} onClick={() => onRunCommand(cmd.id)}>
                <span className="truncate">{cmd.label}</span>
              </DropdownMenuItem>
            ))}
          </DropdownMenuGroup>
        ))}

        {themes.length > 0 && (
          <>
            {commandGroups.length > 0 && <DropdownMenuSeparator />}
            <DropdownMenuLabel className="flex items-center gap-1.5 text-muted-foreground">
              <Palette size={12} />
              {t('pluginLauncher.themes')}
            </DropdownMenuLabel>
            <DropdownMenuItem onClick={() => setActiveTheme(null)}>
              <span className="flex-1 truncate">{t('pluginLauncher.themeDefault')}</span>
              {activeThemeId === null && <Check size={14} className="text-accent" />}
            </DropdownMenuItem>
            {themes.map(theme => (
              <DropdownMenuItem
                key={theme.id}
                onClick={() => setActiveTheme(activeThemeId === theme.id ? null : theme.id)}
              >
                <span className="flex-1 truncate">{theme.name}</span>
                {activeThemeId === theme.id && <Check size={14} className="text-accent" />}
              </DropdownMenuItem>
            ))}
          </>
        )}
      </DropdownMenuContent>
    </DropdownMenu>
  );
}

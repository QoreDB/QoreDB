// SPDX-License-Identifier: Apache-2.0

import { AlertTriangle, Ban, Puzzle, Shield, Trash2, ZapOff } from 'lucide-react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Switch } from '@/components/ui/switch';
import { Tooltip } from '@/components/ui/tooltip';
import {
  type InstalledPlugin,
  PLUGIN_CIRCUIT_BREAKER_THRESHOLD,
  type PluginRuntimeStatus,
} from '@/lib/plugins';
import { requestedCaps } from './ConsentDialog';

interface PluginCardProps {
  plugin: InstalledPlugin;
  status?: PluginRuntimeStatus;
  busy?: boolean;
  onToggle: (enabled: boolean) => void;
  onRemove: () => void;
  onDetails: () => void;
}

export function PluginCard({
  plugin,
  status,
  busy,
  onToggle,
  onRemove,
  onDetails,
}: PluginCardProps) {
  const { t } = useTranslation();
  const { manifest, enabled, compatible } = plugin;
  const c = manifest.contributes;

  // An executable plugin that loaded fine but was granted no capability can't
  // do anything at runtime — surface that instead of presenting it as active.
  const isInert =
    enabled &&
    compatible &&
    !!manifest.runtime &&
    requestedCaps(plugin).length > 0 &&
    !!status?.loaded &&
    status.granted.length === 0;

  // The circuit breaker unloaded it after repeated failures.
  const disabledByErrors =
    enabled &&
    compatible &&
    !!manifest.runtime &&
    !!status &&
    !status.loaded &&
    status.failureCount >= PLUGIN_CIRCUIT_BREAKER_THRESHOLD;

  const badges = [
    c.snippets.length > 0 && t('plugins.contributions.snippets', { count: c.snippets.length }),
    c.connectionTemplates.length > 0 &&
      t('plugins.contributions.connectionTemplates', { count: c.connectionTemplates.length }),
    c.themes.length > 0 && t('plugins.contributions.themes', { count: c.themes.length }),
  ].filter(Boolean) as string[];

  return (
    <div className="flex items-start gap-3 rounded-lg border border-border p-3">
      <div className="mt-0.5 text-muted-foreground">
        <Puzzle size={18} />
      </div>
      <div className="min-w-0 flex-1">
        <div className="flex flex-wrap items-center gap-2">
          <button
            type="button"
            onClick={onDetails}
            className="truncate text-sm font-medium text-foreground hover:underline"
          >
            {manifest.name}
          </button>
          <span className="text-xs text-muted-foreground">v{manifest.version}</span>
          {manifest.runtime && (
            <Tooltip content={t('plugins.card.executableTooltip')}>
              <span className="inline-flex items-center gap-1 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning">
                <Shield size={11} />
                {t('plugins.card.executable')}
              </span>
            </Tooltip>
          )}
          {!compatible && (
            <span className="inline-flex items-center gap-1 text-xs text-warning">
              <AlertTriangle size={12} />
              {t('plugins.card.incompatible')}
            </span>
          )}
          {disabledByErrors && (
            <Tooltip content={t('plugins.card.disabledErrorsTooltip')}>
              <span className="inline-flex items-center gap-1 rounded bg-destructive/15 px-1.5 py-0.5 text-[10px] font-medium text-destructive">
                <Ban size={11} />
                {t('plugins.card.disabledErrors')}
              </span>
            </Tooltip>
          )}
          {isInert && !disabledByErrors && (
            <Tooltip content={t('plugins.card.inertTooltip')}>
              <button
                type="button"
                onClick={onDetails}
                className="inline-flex items-center gap-1 rounded bg-warning/15 px-1.5 py-0.5 text-[10px] font-medium text-warning hover:bg-warning/25"
              >
                <ZapOff size={11} />
                {t('plugins.card.inert')}
              </button>
            </Tooltip>
          )}
        </div>
        {manifest.author && (
          <p className="text-xs text-muted-foreground">
            {t('plugins.card.by', { author: manifest.author })}
          </p>
        )}
        {manifest.description && (
          <p className="mt-0.5 line-clamp-2 text-xs text-muted-foreground">
            {manifest.description}
          </p>
        )}
        {badges.length > 0 && (
          <div className="mt-1.5 flex flex-wrap gap-1">
            {badges.map(b => (
              <span
                key={b}
                className="rounded bg-muted px-1.5 py-0.5 text-[10px] text-muted-foreground"
              >
                {b}
              </span>
            ))}
          </div>
        )}
      </div>
      <div className="flex shrink-0 items-center gap-1.5">
        <Switch
          checked={enabled}
          disabled={busy || !compatible}
          onCheckedChange={onToggle}
          aria-label={t(enabled ? 'plugins.card.disable' : 'plugins.card.enable')}
        />
        <Button
          variant="ghost"
          size="sm"
          disabled={busy}
          onClick={onRemove}
          className="text-muted-foreground hover:text-destructive"
          aria-label={t('plugins.card.remove')}
        >
          <Trash2 size={14} />
        </Button>
      </div>
    </div>
  );
}

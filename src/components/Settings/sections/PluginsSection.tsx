// SPDX-License-Identifier: Apache-2.0

import { Check, Plus, Puzzle } from 'lucide-react';
import { useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { InstallPluginDialog } from '@/components/Plugins/InstallPluginDialog';
import { PluginCard } from '@/components/Plugins/PluginCard';
import { PluginDetailDialog } from '@/components/Plugins/PluginDetailDialog';
import { Button } from '@/components/ui/button';
import { type InstalledPlugin, removePlugin, setPluginEnabled } from '@/lib/plugins';
import { confirmDialog } from '@/lib/stores/confirmStore';
import { usePlugins } from '@/providers/PluginProvider';
import { SettingsCard } from '../SettingsCard';
import { MarketplaceSection } from './MarketplaceSection';

type PluginsTab = 'installed' | 'browse';

interface PluginsSectionProps {
  searchQuery?: string;
}

export function PluginsSection({ searchQuery }: PluginsSectionProps) {
  const { t } = useTranslation();
  const [tab, setTab] = useState<PluginsTab>('installed');

  if (searchQuery) {
    return (
      <>
        <InstalledPluginsTab searchQuery={searchQuery} />
        <MarketplaceSection searchQuery={searchQuery} />
      </>
    );
  }

  return (
    <div className="space-y-3">
      <div className="flex gap-1 border-b border-border">
        <TabButton active={tab === 'installed'} onClick={() => setTab('installed')}>
          {t('plugins.tabs.installed')}
        </TabButton>
        <TabButton active={tab === 'browse'} onClick={() => setTab('browse')}>
          {t('plugins.tabs.browse')}
        </TabButton>
      </div>
      {tab === 'installed' ? <InstalledPluginsTab /> : <MarketplaceSection />}
    </div>
  );
}

function TabButton({
  active,
  onClick,
  children,
}: {
  active: boolean;
  onClick: () => void;
  children: React.ReactNode;
}) {
  return (
    <button
      type="button"
      onClick={onClick}
      className={`relative -mb-px px-3 py-2 text-xs font-medium transition-colors ${
        active
          ? 'border-b-2 border-primary text-foreground'
          : 'border-b-2 border-transparent text-muted-foreground hover:text-foreground'
      }`}
    >
      {children}
    </button>
  );
}

function InstalledPluginsTab({ searchQuery }: { searchQuery?: string }) {
  const { t } = useTranslation();
  const { plugins, contributions, statuses, activeThemeId, setActiveTheme, refresh } = usePlugins();
  const [installOpen, setInstallOpen] = useState(false);
  const [detail, setDetail] = useState<InstalledPlugin | null>(null);
  const [busyId, setBusyId] = useState<string | null>(null);

  async function toggle(plugin: InstalledPlugin, enabled: boolean) {
    setBusyId(plugin.manifest.id);
    try {
      await setPluginEnabled(plugin.manifest.id, enabled);
      if (enabled) AnalyticsService.capture('plugin_enabled');
      await refresh();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  async function remove(plugin: InstalledPlugin) {
    if (
      !(await confirmDialog({
        description: t('plugins.card.removeConfirm', { name: plugin.manifest.name }),
      }))
    )
      return;
    setBusyId(plugin.manifest.id);
    try {
      await removePlugin(plugin.manifest.id);
      AnalyticsService.capture('plugin_removed');
      if (activeThemeId?.startsWith(`${plugin.manifest.id}::`)) {
        setActiveTheme(null);
      }
      toast.success(t('plugins.toast.removed', { name: plugin.manifest.name }));
      await refresh();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setBusyId(null);
    }
  }

  return (
    <>
      <SettingsCard
        id="plugins"
        title={t('plugins.title')}
        description={t('plugins.description')}
        searchQuery={searchQuery}
      >
        <div className="space-y-3">
          <div className="flex justify-end">
            <Button size="sm" onClick={() => setInstallOpen(true)} className="gap-1.5">
              <Plus size={14} />
              {t('plugins.install')}
            </Button>
          </div>

          {plugins.length === 0 ? (
            <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border py-8 text-center">
              <Puzzle size={24} className="text-muted-foreground" />
              <p className="text-sm text-muted-foreground">{t('plugins.empty')}</p>
            </div>
          ) : (
            <div className="space-y-2">
              {plugins.map(p => (
                <PluginCard
                  key={p.manifest.id}
                  plugin={p}
                  status={statuses[p.manifest.id]}
                  busy={busyId === p.manifest.id}
                  onToggle={enabled => toggle(p, enabled)}
                  onRemove={() => remove(p)}
                  onDetails={() => setDetail(p)}
                />
              ))}
            </div>
          )}
        </div>
      </SettingsCard>

      {contributions.themes.length > 0 && (
        <SettingsCard
          id="plugin-themes"
          title={t('plugins.themes.title')}
          description={t('plugins.themes.description')}
          searchQuery={searchQuery}
        >
          <div className="space-y-2">
            {contributions.themes.map(theme => {
              const active = theme.id === activeThemeId;
              return (
                <div
                  key={theme.id}
                  className="flex items-center gap-3 rounded-lg border border-border p-3"
                >
                  <span className="min-w-0 flex-1 truncate text-sm text-foreground">
                    {theme.name}
                  </span>
                  <Button
                    variant={active ? 'ghost' : 'outline'}
                    size="sm"
                    onClick={() => setActiveTheme(active ? null : theme.id)}
                    className="gap-1.5"
                  >
                    {active && <Check size={14} />}
                    {t(active ? 'plugins.themes.active' : 'plugins.themes.apply')}
                  </Button>
                </div>
              );
            })}
          </div>
        </SettingsCard>
      )}

      <InstallPluginDialog open={installOpen} onOpenChange={setInstallOpen} onInstalled={refresh} />
      <PluginDetailDialog
        plugin={detail}
        status={detail ? statuses[detail.manifest.id] : undefined}
        open={detail !== null}
        onOpenChange={o => !o && setDetail(null)}
        onConsentChanged={refresh}
      />
    </>
  );
}

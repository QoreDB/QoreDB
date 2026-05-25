// SPDX-License-Identifier: Apache-2.0

import { Check, Download, ExternalLink, Loader2, RefreshCw, Store } from 'lucide-react';
import { useCallback, useEffect, useMemo, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { toast } from 'sonner';
import { Badge } from '@/components/ui/badge';
import { Button } from '@/components/ui/button';
import { Input } from '@/components/ui/input';
import { installPluginFromUrl } from '@/lib/plugins';
import {
  fetchMarketplaceIndex,
  findLatestVersion,
  MarketplaceError,
  type MarketplaceIndex,
  type MarketplacePlugin,
} from '@/lib/plugins/marketplace';
import { usePlugins } from '@/providers/PluginProvider';
import { SettingsCard } from '../SettingsCard';

interface MarketplaceSectionProps {
  searchQuery?: string;
}

type KindFilter = 'all' | 'declarative' | 'executable';

const MARKETPLACE_URL = 'https://qoredb.com/plugins';

export function MarketplaceSection({ searchQuery }: MarketplaceSectionProps) {
  const { t } = useTranslation();
  const { plugins: installed, refresh } = usePlugins();
  const [index, setIndex] = useState<MarketplaceIndex | null>(null);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);
  const [query, setQuery] = useState('');
  const [kind, setKind] = useState<KindFilter>('all');
  const [installingId, setInstallingId] = useState<string | null>(null);

  const load = useCallback(async () => {
    setLoading(true);
    setError(null);
    try {
      const data = await fetchMarketplaceIndex();
      setIndex(data);
    } catch (err) {
      setError(
        err instanceof MarketplaceError || err instanceof Error
          ? err.message
          : String(err),
      );
    } finally {
      setLoading(false);
    }
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const installedIds = useMemo(
    () => new Set(installed.map(p => p.manifest.id)),
    [installed],
  );

  const filtered = useMemo(() => {
    if (!index) return [];
    const q = query.trim().toLowerCase();
    return index.plugins.filter(p => {
      if (kind !== 'all' && p.kind !== kind) return false;
      if (!q) return true;
      return (
        p.id.toLowerCase().includes(q) ||
        p.name.toLowerCase().includes(q) ||
        (p.description?.toLowerCase().includes(q) ?? false) ||
        (p.author?.toLowerCase().includes(q) ?? false)
      );
    });
  }, [index, query, kind]);

  async function handleInstall(plugin: MarketplacePlugin) {
    const latest = findLatestVersion(plugin);
    if (!latest) return;
    setInstallingId(plugin.id);
    try {
      await installPluginFromUrl(latest.archive.url, latest.archive.sha256);
      toast.success(t('marketplaceSection.toast.installed', { name: plugin.name }));
      await refresh();
    } catch (err) {
      toast.error(err instanceof Error ? err.message : String(err));
    } finally {
      setInstallingId(null);
    }
  }

  return (
    <SettingsCard
      id="marketplace"
      title={t('marketplaceSection.title')}
      description={t('marketplaceSection.description')}
      searchQuery={searchQuery}
    >
      <div className="space-y-3">
        <div className="flex flex-wrap items-center gap-2">
          <div className="relative min-w-0 flex-1">
            <Input
              value={query}
              onChange={e => setQuery(e.target.value)}
              placeholder={t('marketplaceSection.searchPlaceholder')}
              className="h-8"
            />
          </div>
          {(['all', 'declarative', 'executable'] as KindFilter[]).map(k => (
            <Button
              key={k}
              size="sm"
              variant={kind === k ? 'default' : 'outline'}
              onClick={() => setKind(k)}
            >
              {t(`marketplaceSection.filter.${k}`)}
            </Button>
          ))}
          <Button size="sm" variant="outline" onClick={load} disabled={loading}>
            <RefreshCw size={14} className={loading ? 'animate-spin' : ''} />
          </Button>
          <Button size="sm" variant="outline" asChild>
            <a
              href={MARKETPLACE_URL}
              target="_blank"
              rel="noopener noreferrer"
              className="gap-1.5"
            >
              <ExternalLink size={14} />
              {t('marketplaceSection.openOnWeb')}
            </a>
          </Button>
        </div>

        {loading && !index ? (
          <div className="flex items-center justify-center gap-2 rounded-lg border border-dashed border-border py-10 text-sm text-muted-foreground">
            <Loader2 size={14} className="animate-spin" />
            {t('marketplaceSection.loading')}
          </div>
        ) : error ? (
          <div className="rounded-lg border border-dashed border-border bg-muted/30 p-4 text-sm">
            <p className="text-muted-foreground">{t('marketplaceSection.error')}</p>
            <p className="mt-1 text-xs text-muted-foreground">{error}</p>
          </div>
        ) : filtered.length === 0 ? (
          <div className="flex flex-col items-center gap-2 rounded-lg border border-dashed border-border py-8 text-center">
            <Store size={24} className="text-muted-foreground" />
            <p className="text-sm text-muted-foreground">{t('marketplaceSection.empty')}</p>
          </div>
        ) : (
          <div className="space-y-2">
            {filtered.map(plugin => {
              const latest = findLatestVersion(plugin);
              const alreadyInstalled = installedIds.has(plugin.id);
              const isInstalling = installingId === plugin.id;
              return (
                <div
                  key={plugin.id}
                  className="rounded-lg border border-border bg-card p-3"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="text-sm font-medium text-foreground">
                          {plugin.name}
                        </span>
                        <span className="font-mono text-xs text-muted-foreground">
                          {plugin.id}
                        </span>
                        <Badge variant="outline" className="text-[10px]">
                          {t(`marketplaceSection.kind.${plugin.kind}`)}
                        </Badge>
                        {latest?.runtime?.integrity ? null : latest?.runtime ? (
                          <Badge variant="outline">{t('marketplaceSection.unsigned')}</Badge>
                        ) : null}
                      </div>
                      {plugin.description ? (
                        <p className="mt-1 text-xs text-muted-foreground">
                          {plugin.description}
                        </p>
                      ) : null}
                      {latest?.runtime ? (
                        <div className="mt-2 flex flex-wrap gap-1">
                          {latest.runtime.hooks.map(h => (
                            <Badge key={h} variant="outline" className="font-mono text-[10px]">
                              {h}
                            </Badge>
                          ))}
                          {latest.runtime.capabilities.map(c => (
                            <Badge key={c} variant="outline" className="font-mono text-[10px]">
                              {c}
                            </Badge>
                          ))}
                        </div>
                      ) : null}
                    </div>
                    <div className="flex shrink-0 flex-col items-end gap-1.5">
                      <span className="font-mono text-[11px] text-muted-foreground">
                        v{plugin.latestVersion}
                      </span>
                      {alreadyInstalled ? (
                        <Badge variant="secondary" className="gap-1">
                          <Check size={12} />
                          {t('marketplaceSection.installed')}
                        </Badge>
                      ) : (
                        <Button
                          size="sm"
                          onClick={() => handleInstall(plugin)}
                          disabled={isInstalling || !latest}
                          className="gap-1.5"
                        >
                          {isInstalling ? (
                            <Loader2 size={14} className="animate-spin" />
                          ) : (
                            <Download size={14} />
                          )}
                          {t(isInstalling ? 'marketplaceSection.installing' : 'marketplaceSection.install')}
                        </Button>
                      )}
                    </div>
                  </div>
                </div>
              );
            })}
          </div>
        )}
      </div>
    </SettingsCard>
  );
}

// SPDX-License-Identifier: BUSL-1.1

import { ArrowRight, Network, Plus, Zap } from 'lucide-react';
import { useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { DRIVER_ICONS, DRIVER_LABELS, type Driver } from '@/lib/drivers';
import type { FederationSource } from '@/lib/federation';
import { getModifierKey } from '@/utils/platform';

interface FederationEmptyStateProps {
  sources: FederationSource[];
  hasEnoughSources: boolean;
  loading: boolean;
  onAddSource: () => void;
  onTryExample: (query: string) => void;
}

export function FederationEmptyState({
  sources,
  hasEnoughSources,
  loading,
  onAddSource,
  onTryExample,
}: FederationEmptyStateProps) {
  const { t } = useTranslation();

  const exampleQuery = useMemo(() => {
    if (sources.length < 2) return null;
    const a = sources[0];
    const b = sources[1];
    return `SELECT a.*, b.*\nFROM ${a.alias}.<schema>.<table> a\nJOIN ${b.alias}.<schema>.<table> b\n  ON a.id = b.id`;
  }, [sources]);

  if (loading) {
    return (
      <div className="flex-1 flex items-center justify-center">
        <div className="flex items-center gap-3 text-muted-foreground">
          <Zap size={16} className="animate-pulse text-accent" />
          <span className="text-sm">{t('federation.executing')}</span>
        </div>
      </div>
    );
  }

  // Not enough sources — onboarding state
  if (!hasEnoughSources) {
    return (
      <div className="flex-1 flex items-center justify-center p-8">
        <div className="flex flex-col items-center text-center max-w-md gap-5">
          <div className="relative">
            <div className="w-16 h-16 rounded-2xl bg-muted/50 border border-border flex items-center justify-center">
              <Network size={28} className="text-muted-foreground/50" />
            </div>
          </div>

          <div className="space-y-2">
            <h3 className="text-lg font-semibold text-foreground">{t('federation.emptyTitle')}</h3>
            <p className="text-sm text-muted-foreground leading-relaxed">
              {t('federation.emptyNeedSources')}
            </p>
          </div>

          {sources.length === 1 && (
            <div className="flex items-center gap-2 px-3 py-2 rounded-lg bg-muted/30 border border-border text-xs">
              <div className="w-4 h-4 rounded-sm overflow-hidden shrink-0">
                <img
                  src={`/databases/${DRIVER_ICONS[sources[0].driver as Driver]}`}
                  alt={DRIVER_LABELS[sources[0].driver as Driver]}
                  className="w-full h-full object-contain"
                />
              </div>
              <span className="font-medium">{sources[0].display_name}</span>
              <span className="text-muted-foreground">+</span>
              <span className="text-muted-foreground italic">{t('federation.oneMoreNeeded')}</span>
            </div>
          )}

          <Button variant="outline" onClick={onAddSource} className="gap-2">
            <Plus size={14} />
            {t('federation.addSource')}
          </Button>
        </div>
      </div>
    );
  }

  // Enough sources — ready state with example
  return (
    <div className="flex-1 flex items-center justify-center p-8">
      <div className="flex flex-col items-center text-center max-w-lg gap-6">
        <div className="relative">
          <div className="w-16 h-16 rounded-2xl bg-accent/10 border border-accent/20 flex items-center justify-center">
            <Network size={28} className="text-accent" />
          </div>
        </div>

        <div className="space-y-2">
          <h3 className="text-lg font-semibold text-foreground">{t('federation.readyTitle')}</h3>
          <p className="text-sm text-muted-foreground leading-relaxed max-w-sm">
            {t('federation.readyDescription')}
          </p>
        </div>

        {/* Connected sources visual */}
        <div className="flex items-center gap-2 flex-wrap justify-center">
          {sources.map((source, i) => (
            <div key={source.alias} className="flex items-center gap-1.5">
              {i > 0 && <span className="text-muted-foreground/40 text-xs mx-1">+</span>}
              <div className="flex items-center gap-1.5 px-2 py-1 rounded-md bg-muted/30 border border-border text-xs">
                <div className="w-3.5 h-3.5 rounded-sm overflow-hidden shrink-0">
                  <img
                    src={`/databases/${DRIVER_ICONS[source.driver as Driver]}`}
                    alt={DRIVER_LABELS[source.driver as Driver]}
                    className="w-full h-full object-contain"
                  />
                </div>
                <span className="font-medium">{source.display_name}</span>
                <span className="font-mono text-[10px] text-muted-foreground/60">
                  {source.alias}
                </span>
              </div>
            </div>
          ))}
        </div>

        {/* Example query */}
        {exampleQuery && (
          <div className="w-full max-w-md">
            <div className="flex items-center justify-between mb-2">
              <span className="text-[10px] font-medium uppercase tracking-wider text-muted-foreground">
                {t('federation.exampleQuery')}
              </span>
            </div>
            <div className="relative group">
              <pre className="text-left text-xs font-mono bg-muted/20 border border-border rounded-lg p-4 text-muted-foreground leading-relaxed overflow-x-auto">
                {exampleQuery}
              </pre>
              <Button
                variant="outline"
                size="sm"
                className="absolute bottom-2 right-2 gap-1.5 opacity-0 group-hover:opacity-100 transition-opacity text-xs h-7"
                onClick={() => onTryExample(exampleQuery)}
              >
                {t('federation.useTemplate')}
                <ArrowRight size={12} />
              </Button>
            </div>
          </div>
        )}

        <div className="flex items-center gap-4 text-[11px] text-muted-foreground/60">
          <span>{t('federation.hintClickSource')}</span>
          <span className="w-px h-3 bg-border" />
          <span>
            {getModifierKey()}+Enter {t('federation.hintToExecute')}
          </span>
        </div>
      </div>
    </div>
  );
}

// SPDX-License-Identifier: Apache-2.0

import { openUrl } from '@tauri-apps/plugin-opener';
import type { LucideIcon } from 'lucide-react';
import {
  BarChart3,
  Box,
  Braces,
  CheckCircle2,
  ExternalLink,
  FileSpreadsheet,
  GitCompare,
  History,
  Layers,
  Library,
  Link2,
  Network,
  ShieldCheck,
  Sparkles,
  Table2,
} from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import { Dialog, DialogContent, DialogDescription, DialogTitle } from '@/components/ui/dialog';
import type { ProFeature } from '@/lib/license';
import { trackProEvent } from '@/lib/licenseTracking';
import { getCheckoutUrl, getPricingUrl, PRO_PRICE_LABEL } from '@/lib/pricing';

const ACCENT = '#6B5CFF';

interface FeatureEntry {
  id: ProFeature;
  icon: LucideIcon;
}

const PRO_FEATURES: FeatureEntry[] = [
  { id: 'sandbox', icon: Box },
  { id: 'visual_diff', icon: GitCompare },
  { id: 'ai', icon: Sparkles },
  { id: 'data_time_travel', icon: History },
  { id: 'data_contracts', icon: Braces },
  { id: 'instant_api', icon: Layers },
  { id: 'audit_advanced', icon: ShieldCheck },
  { id: 'profiling', icon: BarChart3 },
  { id: 'export_xlsx', icon: FileSpreadsheet },
  { id: 'export_parquet', icon: FileSpreadsheet },
  { id: 'custom_safety_rules', icon: ShieldCheck },
  { id: 'query_library_advanced', icon: Library },
  { id: 'virtual_relations_auto_suggest', icon: Link2 },
  { id: 'bulk_edit_unlimited', icon: Table2 },
  { id: 'er_diagram', icon: Network },
];

interface ProDiscoveryPanelProps {
  open: boolean;
  onClose: () => void;
  source?: string;
  onActivate?: () => void;
}

export function ProDiscoveryPanel({ open, onClose, source, onActivate }: ProDiscoveryPanelProps) {
  const { t } = useTranslation();

  useEffect(() => {
    if (open) trackProEvent('pro_discovery_opened', { source });
  }, [open, source]);

  const handleUnlock = () => {
    trackProEvent('pro_upgrade_cta_clicked', { source: source ?? 'discovery_panel' });
    openUrl(getCheckoutUrl());
  };

  const handleLearnMore = () => {
    trackProEvent('pro_upgrade_learn_more_clicked', { source: source ?? 'discovery_panel' });
    openUrl(getPricingUrl());
  };

  const handleFeatureClick = (feature: ProFeature) => {
    trackProEvent('pro_discovery_feature_clicked', {
      feature,
      source: source ?? 'discovery_panel',
    });
    openUrl(getPricingUrl(feature));
  };

  return (
    <Dialog open={open} onOpenChange={value => !value && onClose()}>
      <DialogContent className="max-w-3xl gap-0 p-0">
        <div
          className="border-b px-6 py-5"
          style={{
            background:
              'linear-gradient(180deg, rgba(107, 92, 255, 0.08) 0%, rgba(107, 92, 255, 0.01) 100%)',
            borderColor: 'rgba(107, 92, 255, 0.2)',
          }}
        >
          <DialogTitle className="text-lg font-semibold">
            {t('proDiscovery.title', 'QoreDB Pro — Built for serious work')}
          </DialogTitle>
          <DialogDescription className="mt-1 text-sm">
            {t(
              'proDiscovery.subtitle',
              'Free is great for solo projects and exploration. Pro unlocks the features you need for production-grade database work.'
            )}
          </DialogDescription>
        </div>

        <div className="grid max-h-[60vh] grid-cols-1 gap-2 overflow-y-auto p-4 sm:grid-cols-2">
          {PRO_FEATURES.map(({ id, icon: Icon }) => {
            const title = t(`license.upgrade.features.${id}.title`, {
              defaultValue: id,
            });
            const description = t(`license.upgrade.features.${id}.description`, {
              defaultValue: '',
            });
            const bullets = [
              t(`license.upgrade.features.${id}.bullet1`, { defaultValue: '' }),
              t(`license.upgrade.features.${id}.bullet2`, { defaultValue: '' }),
              t(`license.upgrade.features.${id}.bullet3`, { defaultValue: '' }),
            ].filter(Boolean);

            return (
              <button
                key={id}
                type="button"
                onClick={() => handleFeatureClick(id)}
                className="flex flex-col gap-2 rounded-md border p-3 text-left transition-colors hover:bg-muted/40"
                style={{ borderColor: 'var(--color-border)' }}
              >
                <div className="flex items-center gap-2">
                  <Icon size={16} style={{ color: ACCENT }} aria-hidden />
                  <span className="text-sm font-medium text-(--color-text-primary)">{title}</span>
                </div>
                {description && (
                  <p className="text-xs text-(--color-text-secondary)">{description}</p>
                )}
                {bullets.length > 0 && (
                  <ul className="flex flex-col gap-1">
                    {bullets.slice(0, 2).map(b => (
                      <li
                        key={b}
                        className="flex items-start gap-1.5 text-[11px] text-(--color-text-tertiary)"
                      >
                        <CheckCircle2
                          size={11}
                          className="mt-0.5 shrink-0"
                          style={{ color: ACCENT }}
                          aria-hidden
                        />
                        <span>{b}</span>
                      </li>
                    ))}
                  </ul>
                )}
              </button>
            );
          })}
        </div>

        <div className="flex flex-col gap-3 border-t bg-muted/20 px-6 py-4 sm:flex-row sm:items-center sm:justify-between">
          <div className="flex flex-col text-xs text-(--color-text-secondary)">
            <span className="text-sm font-semibold text-(--color-text-primary)">
              {t('proDiscovery.priceTitle', 'One-time payment, lifetime access')}
            </span>
            <span>
              {t('license.upgrade.priceLine', '{{price}} — perpetual, no subscription', {
                price: PRO_PRICE_LABEL,
              })}
            </span>
          </div>
          <div className="flex flex-wrap items-center gap-2">
            {onActivate && (
              <Button
                variant="ghost"
                size="sm"
                onClick={() => {
                  onActivate();
                  onClose();
                }}
                className="text-xs"
              >
                {t('proDiscovery.alreadyHaveKey', 'I have a license key')}
              </Button>
            )}
            <Button
              variant="ghost"
              size="sm"
              onClick={handleLearnMore}
              className="gap-1 text-xs"
              style={{ color: ACCENT }}
            >
              {t('license.upgrade.learnMore', 'Learn more')}
            </Button>
            <Button
              size="sm"
              onClick={handleUnlock}
              className="gap-1.5 text-xs"
              style={{ background: ACCENT, color: 'white' }}
            >
              {t('license.upgrade.unlock', 'Unlock Pro')}
              <ExternalLink size={12} aria-hidden />
            </Button>
          </div>
        </div>
      </DialogContent>
    </Dialog>
  );
}

// SPDX-License-Identifier: Apache-2.0

import { openUrl } from '@tauri-apps/plugin-opener';
import type { LucideIcon } from 'lucide-react';
import {
  BarChart3,
  Box,
  Braces,
  Bug,
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
  X,
} from 'lucide-react';
import { useEffect, useState } from 'react';
import { useTranslation } from 'react-i18next';
import { Button } from '@/components/ui/button';
import type { ProFeature } from '@/lib/license';
import { featureRequiredTier } from '@/lib/license';
import { dismissPrompt, isPromptDismissed, trackProEvent } from '@/lib/licenseTracking';
import { getCheckoutUrl, getPricingUrl } from '@/lib/pricing';
import { LicenseBadge } from './LicenseBadge';

const ACCENT = 'var(--color-accent)';
const ACCENT_BORDER = 'color-mix(in srgb, var(--color-accent) 25%, transparent)';
const ACCENT_BG_SUBTLE = 'color-mix(in srgb, var(--color-accent) 4%, transparent)';
const ACCENT_BG_SOFT = 'color-mix(in srgb, var(--color-accent) 12%, transparent)';
const ACCENT_GRADIENT =
  'linear-gradient(180deg, color-mix(in srgb, var(--color-accent) 5%, transparent) 0%, color-mix(in srgb, var(--color-accent) 1%, transparent) 100%)';

const FEATURE_ICONS: Record<ProFeature, LucideIcon> = {
  sandbox: Box,
  visual_diff: GitCompare,
  er_diagram: Network,
  audit_advanced: ShieldCheck,
  profiling: BarChart3,
  ai: Sparkles,
  export_xlsx: FileSpreadsheet,
  export_parquet: FileSpreadsheet,
  custom_safety_rules: ShieldCheck,
  query_library_advanced: Library,
  virtual_relations_auto_suggest: Link2,
  data_time_travel: History,
  bulk_edit_unlimited: Table2,
  data_contracts: Braces,
  instant_api: Layers,
};

const FEATURE_FALLBACK_ICON: LucideIcon = Bug;

interface UpgradePromptProps {
  feature: ProFeature;
  className?: string;
  variant?: 'inline' | 'compact';
  source?: string;
  hideIfDismissed?: boolean;
}

/**
 * Contextual upgrade prompt shown when a gated feature is accessed.
 * Tracks engagement events. Follows the Design DNA: no blocking modal, no flashy animation.
 */
export function UpgradePrompt({
  feature,
  className,
  variant = 'inline',
  source,
  hideIfDismissed = false,
}: UpgradePromptProps) {
  const { t } = useTranslation();
  const [locallyDismissed, setLocallyDismissed] = useState(false);
  const persistedDismissed = isPromptDismissed(feature);
  const hidden = locallyDismissed || (hideIfDismissed && persistedDismissed);

  useEffect(() => {
    if (hidden) return;
    trackProEvent('pro_upgrade_prompt_seen', { feature, source });
  }, [feature, source, hidden]);

  if (hidden) return null;

  const requiredTier = featureRequiredTier(feature);
  const Icon = FEATURE_ICONS[feature] ?? FEATURE_FALLBACK_ICON;

  const title = t(`license.upgrade.features.${feature}.title`, {
    defaultValue: t('license.upgrade.defaultTitle', 'Unlock this feature with QoreDB Pro'),
  });
  const description = t(`license.upgrade.features.${feature}.description`, {
    defaultValue: t(
      `license.features.${feature}`,
      t(
        'license.upgrade.defaultDescription',
        'This feature is part of QoreDB Pro — built for serious database work.'
      )
    ),
  });

  const bullets = [
    t(`license.upgrade.features.${feature}.bullet1`, { defaultValue: '' }),
    t(`license.upgrade.features.${feature}.bullet2`, { defaultValue: '' }),
    t(`license.upgrade.features.${feature}.bullet3`, { defaultValue: '' }),
  ].filter(Boolean);

  const handleUnlock = () => {
    trackProEvent('pro_upgrade_cta_clicked', { feature, source });
    openUrl(getCheckoutUrl(feature));
  };

  const handleLearnMore = () => {
    trackProEvent('pro_upgrade_learn_more_clicked', { feature, source });
    openUrl(getPricingUrl(feature));
  };

  const handleDismiss = () => {
    dismissPrompt(feature);
    setLocallyDismissed(true);
  };

  if (variant === 'compact') {
    return (
      <section
        className={`flex items-center gap-3 rounded-md border px-3 py-2 text-sm ${className ?? ''}`}
        style={{
          borderColor: ACCENT_BORDER,
          background: ACCENT_BG_SUBTLE,
        }}
        aria-label={title}
      >
        <Icon size={16} style={{ color: ACCENT }} aria-hidden />
        <span className="flex-1 text-(--color-text-secondary)">{title}</span>
        <Button
          size="sm"
          variant="ghost"
          className="h-7 gap-1 px-2 text-xs"
          style={{ color: ACCENT }}
          onClick={handleUnlock}
        >
          {t('license.upgrade.unlock', 'Unlock Pro')}
          <ExternalLink size={11} aria-hidden />
        </Button>
        <button
          type="button"
          onClick={handleDismiss}
          aria-label={t('license.upgrade.dismiss', 'Dismiss')}
          className="text-muted-foreground/60 hover:text-foreground transition-colors"
        >
          <X size={14} />
        </button>
      </section>
    );
  }

  return (
    <section
      className={`relative flex flex-col gap-4 overflow-hidden rounded-lg border p-6 ${className ?? ''}`}
      style={{
        borderColor: ACCENT_BORDER,
        background: ACCENT_GRADIENT,
      }}
      aria-label={title}
    >
      <button
        type="button"
        onClick={handleDismiss}
        aria-label={t('license.upgrade.dismiss', 'Don’t ask again for this feature')}
        className="absolute right-3 top-3 text-muted-foreground/50 hover:text-foreground transition-colors"
      >
        <X size={14} />
      </button>

      <div className="flex items-start gap-3">
        <div
          className="flex h-10 w-10 shrink-0 items-center justify-center rounded-md"
          style={{ background: ACCENT_BG_SOFT, color: ACCENT }}
        >
          <Icon size={20} aria-hidden />
        </div>
        <div className="flex min-w-0 flex-col gap-1">
          <div className="flex flex-wrap items-center gap-2">
            <h3 className="text-sm font-semibold text-(--color-text-primary)">{title}</h3>
            <LicenseBadge tier={requiredTier} />
          </div>
          <p className="text-xs text-(--color-text-secondary)">{description}</p>
        </div>
      </div>

      {bullets.length > 0 && (
        <ul className="flex flex-col gap-1.5 pl-1">
          {bullets.map(bullet => (
            <li
              key={bullet}
              className="flex items-start gap-2 text-xs text-(--color-text-secondary)"
            >
              <CheckCircle2
                size={13}
                className="mt-0.5 shrink-0"
                style={{ color: ACCENT }}
                aria-hidden
              />
              <span>{bullet}</span>
            </li>
          ))}
        </ul>
      )}

      <div className="flex flex-wrap items-center gap-2 pt-1">
        <Button
          size="sm"
          onClick={handleUnlock}
          className="gap-1.5 text-xs"
          style={{ background: ACCENT, color: 'white' }}
        >
          {t('license.upgrade.unlock', 'Unlock Pro')}
          <ExternalLink size={12} aria-hidden />
        </Button>
        <Button
          variant="ghost"
          size="sm"
          onClick={handleLearnMore}
          className="gap-1.5 px-2 text-xs"
          style={{ color: ACCENT }}
        >
          {t('license.upgrade.learnMore', 'Learn more')}
        </Button>
      </div>
    </section>
  );
}

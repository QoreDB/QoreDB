// SPDX-License-Identifier: Apache-2.0

import type { TFunction } from 'i18next';
import { Sparkles } from 'lucide-react';
import { toast } from 'sonner';
import type { LicenseTier } from '@/lib/license';
import { trackProEvent } from '@/lib/licenseTracking';
import { setProDiscoveryOpen } from '@/lib/stores/modalStore';
import {
  getQueryCount,
  getUsageBannerThreshold,
  incrementQueryCount,
  markUsageBannerShown,
} from '@/lib/usageCounter';

/**
 * Records a query execution and, when relevant, shows a discreet usage banner
 * to Core users encouraging them to discover Pro.
 *
 * No-op for Pro/Team/Enterprise users — they shouldn't be nagged.
 * No-op if the threshold has been shown or the cooldown hasn't elapsed.
 */
export function recordQueryAndMaybeNotify(tier: LicenseTier, t: TFunction): void {
  const count = incrementQueryCount();
  if (tier !== 'core') return;

  const threshold = getUsageBannerThreshold(count);
  if (threshold == null) return;

  markUsageBannerShown(threshold);
  trackProEvent('pro_upgrade_prompt_seen', {
    source: `usage_banner_${threshold}`,
  });

  toast(
    t('usageBanner.title', "You've run {{count}} queries with QoreDB", { count: getQueryCount() }),
    {
      description: t(
        'usageBanner.description',
        'Support the project and unlock the Pro features built for serious work.'
      ),
      duration: 12000,
      icon: <Sparkles size={16} style={{ color: '#6B5CFF' }} />,
      action: {
        label: t('usageBanner.cta', 'Discover Pro'),
        onClick: () => {
          trackProEvent('pro_upgrade_cta_clicked', {
            source: `usage_banner_${threshold}`,
          });
          setProDiscoveryOpen(true);
        },
      },
    }
  );
}

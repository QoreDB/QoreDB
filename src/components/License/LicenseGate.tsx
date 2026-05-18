// SPDX-License-Identifier: Apache-2.0

import { type ReactNode, useEffect } from 'react';
import type { ProFeature } from '@/lib/license';
import { trackProEvent } from '@/lib/licenseTracking';
import { useLicense } from '@/providers/LicenseProvider';
import { UpgradePrompt } from './UpgradePrompt';

interface LicenseGateProps {
  feature: ProFeature;
  children: ReactNode;
  fallback?: ReactNode;
  source?: string;
}

/**
 * Conditionally renders children if the feature is unlocked,
 * otherwise shows a fallback (defaults to UpgradePrompt).
 *
 * Fires `pro_feature_seen` when unlocked, `pro_feature_blocked` when gated.
 */
export function LicenseGate({ feature, children, fallback, source }: LicenseGateProps) {
  const { isFeatureEnabled } = useLicense();
  const enabled = isFeatureEnabled(feature);

  useEffect(() => {
    trackProEvent(enabled ? 'pro_feature_seen' : 'pro_feature_blocked', { feature, source });
  }, [enabled, feature, source]);

  if (enabled) return <>{children}</>;
  return <>{fallback ?? <UpgradePrompt feature={feature} source={source} />}</>;
}

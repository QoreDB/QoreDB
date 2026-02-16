import type { ReactNode } from 'react';
import type { ProFeature } from '@/lib/license';
import { useLicense } from '@/providers/LicenseProvider';
import { UpgradePrompt } from './UpgradePrompt';

interface LicenseGateProps {
  feature: ProFeature;
  children: ReactNode;
  fallback?: ReactNode;
}

/**
 * Conditionally renders children if the feature is unlocked,
 * otherwise shows a fallback (defaults to UpgradePrompt).
 */
export function LicenseGate({ feature, children, fallback }: LicenseGateProps) {
  const { isFeatureEnabled } = useLicense();

  if (isFeatureEnabled(feature)) {
    return <>{children}</>;
  }

  return <>{fallback ?? <UpgradePrompt feature={feature} />}</>;
}

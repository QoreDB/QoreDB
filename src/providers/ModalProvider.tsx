// SPDX-License-Identifier: Apache-2.0

import { type ReactNode, useEffect } from 'react';
import { AnalyticsService } from '@/components/Onboarding/AnalyticsService';
import { setShowOnboarding } from '@/lib/stores/modalStore';

/**
 * Thin wrapper that runs one-time initialization side effects.
 * All modal state lives in the external modalStore (useSyncExternalStore),
 * so this provider does NOT cause re-render cascades on state changes.
 */
export function ModalProvider({ children }: { children: ReactNode }) {
  useEffect(() => {
    if (!AnalyticsService.isOnboardingCompleted()) {
      setShowOnboarding(true);
    }
  }, []);

  return <>{children}</>;
}

// SPDX-License-Identifier: Apache-2.0

import { createContext, useContext, useEffect, useState, useCallback, type ReactNode } from 'react';
import type { LicenseStatus, LicenseTier, ProFeature } from '@/lib/license';
import {
  getLicenseStatus,
  activateLicense,
  deactivateLicense,
  isFeatureEnabled as checkFeature,
} from '@/lib/license';

export interface LicenseContextValue {
  status: LicenseStatus;
  loading: boolean;
  tier: LicenseTier;
  isFeatureEnabled: (feature: ProFeature) => boolean;
  activate: (key: string) => Promise<LicenseStatus>;
  deactivate: () => Promise<void>;
}

const DEFAULT_STATUS: LicenseStatus = {
  tier: 'core',
  email: null,
  issued_at: null,
  expires_at: null,
  is_expired: false,
};

const LicenseContext = createContext<LicenseContextValue | null>(null);

export function LicenseProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<LicenseStatus>(DEFAULT_STATUS);
  const [loading, setLoading] = useState(true);

  useEffect(() => {
    getLicenseStatus()
      .then(setStatus)
      .catch(() => {
        // Silently fall back to Core on error
      })
      .finally(() => setLoading(false));
  }, []);

  const activate = useCallback(async (key: string): Promise<LicenseStatus> => {
    const result = await activateLicense(key);
    setStatus(result);
    return result;
  }, []);

  const deactivate = useCallback(async () => {
    await deactivateLicense();
    setStatus(DEFAULT_STATUS);
  }, []);

  const isEnabled = useCallback(
    (feature: ProFeature) => checkFeature(status.tier, feature),
    [status.tier]
  );

  return (
    <LicenseContext.Provider
      value={{
        status,
        loading,
        tier: status.tier,
        isFeatureEnabled: isEnabled,
        activate,
        deactivate,
      }}
    >
      {children}
    </LicenseContext.Provider>
  );
}

export function useLicense(): LicenseContextValue {
  const ctx = useContext(LicenseContext);
  if (!ctx) throw new Error('useLicense must be used within LicenseProvider');
  return ctx;
}

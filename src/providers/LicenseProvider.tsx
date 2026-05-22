// SPDX-License-Identifier: Apache-2.0

import { createContext, type ReactNode, useCallback, useContext, useEffect, useState } from 'react';
import type { LicenseStatus, LicenseTier, ProFeature } from '@/lib/license';
import {
  activateLicense,
  isFeatureEnabled as checkFeature,
  deactivateLicense,
  devSetLicenseTier,
  getLicenseStatus,
} from '@/lib/license';

export interface LicenseContextValue {
  status: LicenseStatus;
  loading: boolean;
  tier: LicenseTier;
  proActivation: ProActivationEvent | null;
  isFeatureEnabled: (feature: ProFeature) => boolean;
  activate: (key: string) => Promise<LicenseStatus>;
  deactivate: () => Promise<void>;
  dismissProActivation: () => void;
  /** Dev-only: override the tier. null to clear. */
  devSetTier: (tier: LicenseTier | null) => Promise<void>;
}

export interface ProActivationEvent {
  id: number;
  tier: Exclude<LicenseTier, 'core'>;
  email: string | null;
}

const DEFAULT_STATUS: LicenseStatus = {
  tier: 'core',
  email: null,
  payment_id: null,
  issued_at: null,
  expires_at: null,
  is_expired: false,
  is_founder: false,
};

const LicenseContext = createContext<LicenseContextValue | null>(null);

function activePaidTier(status: LicenseStatus): Exclude<LicenseTier, 'core'> | null {
  if (status.tier === 'core' || status.is_expired) return null;
  return status.tier;
}

export function LicenseProvider({ children }: { children: ReactNode }) {
  const [status, setStatus] = useState<LicenseStatus>(DEFAULT_STATUS);
  const [loading, setLoading] = useState(true);
  const [proActivation, setProActivation] = useState<ProActivationEvent | null>(null);

  useEffect(() => {
    getLicenseStatus()
      .then(setStatus)
      .catch(() => {
        // Silently fall back to Core on error
      })
      .finally(() => setLoading(false));
  }, []);

  const activate = useCallback(
    async (key: string): Promise<LicenseStatus> => {
      const previousTier = activePaidTier(status);
      const result = await activateLicense(key);
      const nextTier = activePaidTier(result);
      setStatus(result);
      if (!previousTier && nextTier) {
        setProActivation({
          id: Date.now(),
          tier: nextTier,
          email: result.email,
        });
      }
      return result;
    },
    [status]
  );

  const deactivate = useCallback(async () => {
    await deactivateLicense();
    setStatus(DEFAULT_STATUS);
    setProActivation(null);
  }, []);

  const devSetTier = useCallback(
    async (tier: LicenseTier | null) => {
      try {
        const previousTier = activePaidTier(status);
        const result = await devSetLicenseTier(tier);
        const nextTier = activePaidTier(result);
        setStatus(result);
        if (!previousTier && nextTier) {
          setProActivation({
            id: Date.now(),
            tier: nextTier,
            email: result.email,
          });
        }
      } catch {
        // Silently fail in release builds
      }
    },
    [status]
  );

  const dismissProActivation = useCallback(() => {
    setProActivation(null);
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
        proActivation,
        isFeatureEnabled: isEnabled,
        activate,
        deactivate,
        dismissProActivation,
        devSetTier,
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

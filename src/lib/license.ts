// SPDX-License-Identifier: Apache-2.0

/**
 * License types and Tauri bindings for the Open Core licensing system.
 */
import { invoke } from '@/lib/transport';

export type LicenseTier = 'core' | 'pro' | 'team' | 'enterprise';

export interface LicenseStatus {
  tier: LicenseTier;
  email: string | null;
  payment_id: string | null;
  issued_at: string | null;
  expires_at: string | null;
  is_expired: boolean;
  seats: number | null;
  is_founder: boolean;
}

export type ProFeature =
  | 'sandbox'
  | 'visual_diff'
  | 'er_diagram'
  | 'audit_advanced'
  | 'profiling'
  | 'ai'
  | 'export_xlsx'
  | 'export_parquet'
  | 'custom_safety_rules'
  | 'query_library_advanced'
  | 'virtual_relations_auto_suggest'
  | 'data_time_travel'
  | 'bulk_edit_unlimited'
  | 'data_contracts'
  | 'instant_api'
  | 'data_generator';

const TIER_LEVELS: Record<LicenseTier, number> = {
  core: 0,
  pro: 1,
  team: 2,
  enterprise: 3,
};

const FEATURE_REQUIRED_TIER: Record<ProFeature, LicenseTier> = {
  sandbox: 'pro',
  visual_diff: 'pro',
  er_diagram: 'core',
  audit_advanced: 'pro',
  profiling: 'pro',
  ai: 'pro',
  export_xlsx: 'pro',
  export_parquet: 'pro',
  custom_safety_rules: 'pro',
  query_library_advanced: 'pro',
  virtual_relations_auto_suggest: 'pro',
  data_time_travel: 'pro',
  bulk_edit_unlimited: 'pro',
  data_contracts: 'pro',
  instant_api: 'pro',
  data_generator: 'pro',
};

/** Returns true if `current` tier includes features of `required` tier. */
export function tierIncludes(current: LicenseTier, required: LicenseTier): boolean {
  return TIER_LEVELS[current] >= TIER_LEVELS[required];
}

/** Returns the minimum tier needed for a given feature. */
export function featureRequiredTier(feature: ProFeature): LicenseTier {
  return FEATURE_REQUIRED_TIER[feature];
}

/** Checks whether a feature is enabled for the given tier. */
export function isFeatureEnabled(tier: LicenseTier, feature: ProFeature): boolean {
  return tierIncludes(tier, featureRequiredTier(feature));
}

export function licenseErrorKey(raw: unknown): string {
  const msg = String(raw);
  if (
    msg.startsWith('INVALID_BASE64') ||
    msg.startsWith('INVALID_JSON') ||
    msg.startsWith('INVALID_FORMAT') ||
    msg.startsWith('INVALID_SIGNATURE')
  ) {
    return 'license.errors.invalidKey';
  }
  if (msg.startsWith('EXPIRED_LICENSE')) return 'license.errors.expired';
  if (msg.startsWith('UNSUPPORTED_TIER')) return 'license.errors.unsupportedTier';
  if (msg.startsWith('Storage error')) return 'license.errors.storage';
  if (msg.startsWith('NO_ACTIVE_SUBSCRIPTION')) return 'license.errors.noSubscription';
  if (msg.startsWith('REFRESH_REQUEST_FAILED') || msg.startsWith('PORTAL_REQUEST_FAILED')) {
    return 'license.errors.network';
  }
  return 'license.errors.generic';
}

export async function activateLicense(key: string): Promise<LicenseStatus> {
  return invoke('activate_license', { key });
}

export async function getLicenseStatus(): Promise<LicenseStatus> {
  return invoke('get_license_status');
}

export async function deactivateLicense(): Promise<void> {
  return invoke('deactivate_license');
}

/** Fetches the up-to-date key from the site and re-activates it (Team renewal/seat change). */
export async function refreshLicense(): Promise<LicenseStatus> {
  return invoke('refresh_license');
}

/** Returns a Stripe billing portal URL for the active license's email. */
export async function getBillingPortalUrl(): Promise<string> {
  return invoke('get_billing_portal_url');
}

/** Dev-only: override the license tier. Pass null to clear. */
export async function devSetLicenseTier(tier: LicenseTier | null): Promise<LicenseStatus> {
  return invoke('dev_set_license_tier', { tier });
}

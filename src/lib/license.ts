// SPDX-License-Identifier: Apache-2.0

/**
 * License types and Tauri bindings for the Open Core licensing system.
 */
import { invoke } from '@tauri-apps/api/core';

// ============================================
// TYPES (mirror Rust enums in license/status.rs)
// ============================================

export type LicenseTier = 'core' | 'pro' | 'team' | 'enterprise';

export interface LicenseStatus {
  tier: LicenseTier;
  email: string | null;
  payment_id: string | null;
  issued_at: string | null;
  expires_at: string | null;
  is_expired: boolean;
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
  | 'virtual_relations_auto_suggest';

// ============================================
// TIER UTILITIES
// ============================================

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

// ============================================
// TAURI COMMANDS
// ============================================

export async function activateLicense(key: string): Promise<LicenseStatus> {
  return invoke('activate_license', { key });
}

export async function getLicenseStatus(): Promise<LicenseStatus> {
  return invoke('get_license_status');
}

export async function deactivateLicense(): Promise<void> {
  return invoke('deactivate_license');
}

/** Dev-only: override the license tier. Pass null to clear. */
export async function devSetLicenseTier(tier: LicenseTier | null): Promise<LicenseStatus> {
  return invoke('dev_set_license_tier', { tier });
}

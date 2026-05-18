// SPDX-License-Identifier: Apache-2.0

import type { ProFeature } from '@/lib/license';

export const PRO_PRICE_EUR = 49;
export const PRO_PRICE_LABEL = `${PRO_PRICE_EUR}€`;

const SITE_BASE = 'https://www.qoredb.com';

const FEATURE_ANCHORS: Record<ProFeature, string> = {
  sandbox: 'sandbox',
  visual_diff: 'diff',
  er_diagram: 'er-diagram',
  audit_advanced: 'audit',
  profiling: 'profiling',
  ai: 'ai',
  export_xlsx: 'export',
  export_parquet: 'export',
  custom_safety_rules: 'safety',
  query_library_advanced: 'library',
  virtual_relations_auto_suggest: 'virtual-relations',
  data_time_travel: 'time-travel',
  bulk_edit_unlimited: 'bulk-edit',
  data_contracts: 'contracts',
  instant_api: 'instant-api',
};

export function getPricingUrl(feature?: ProFeature): string {
  if (!feature) return `${SITE_BASE}/pricing`;
  return `${SITE_BASE}/pricing#${FEATURE_ANCHORS[feature]}`;
}

export function getCheckoutUrl(feature?: ProFeature): string {
  const anchor = feature ? FEATURE_ANCHORS[feature] : '';
  const params = new URLSearchParams({ tier: 'pro' });
  if (anchor) params.set('feature', anchor);
  return `${SITE_BASE}/checkout?${params.toString()}`;
}

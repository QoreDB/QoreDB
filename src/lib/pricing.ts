// SPDX-License-Identifier: Apache-2.0

import i18n from '@/i18n';
import type { ProFeature } from '@/lib/license';

export const PRO_PRICE_EUR = 49;
export const PRO_PRICE_LABEL = `${PRO_PRICE_EUR}€`;

const SITE_BASE = 'https://www.qoredb.com';

/**
 * Locales supported by the marketing site. Anything outside this list
 * falls back to English to avoid 404s.
 */
const SITE_LOCALES = new Set(['en', 'fr']);
const DEFAULT_SITE_LOCALE = 'en';

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
  data_generator: 'data-generator',
  index_suggestions: 'index-suggestions',
};

function getSiteLocale(): string {
  const lang = (i18n.language ?? '').toLowerCase().split('-')[0];
  return SITE_LOCALES.has(lang) ? lang : DEFAULT_SITE_LOCALE;
}

/**
 * Pricing page URL on the marketing site. Pro purchases (Stripe Checkout)
 * are initiated from this page — there is no separate /checkout endpoint.
 */
export function getPricingUrl(feature?: ProFeature): string {
  const base = `${SITE_BASE}/${getSiteLocale()}/pricing`;
  if (!feature) return base;
  return `${base}#${FEATURE_ANCHORS[feature]}`;
}

/**
 * Alias kept for backward-compat with call sites that previously used
 * a dedicated checkout URL. Today both resolve to the pricing page,
 * where the Stripe button lives.
 */
export const getCheckoutUrl = getPricingUrl;

// SPDX-License-Identifier: Apache-2.0

/**
 * QoreDB shipped an opt-in PostHog integration until v0.1.x. Users who had
 * opted in still carry its persisted identity (`ph_*`, which holds a
 * distinct_id) and our own opt-in flags. Removing the SDK does not remove
 * what it already wrote to disk, so we clear it on startup.
 */
const LEGACY_KEYS = ['qoredb_analytics_enabled'];
const LEGACY_PREFIXES = ['qoredb_daily_event::', 'ph_'];

export function purgeLegacyTelemetryState(): void {
  try {
    const stale = Object.keys(localStorage).filter(
      key => LEGACY_KEYS.includes(key) || LEGACY_PREFIXES.some(prefix => key.startsWith(prefix))
    );
    for (const key of stale) {
      localStorage.removeItem(key);
    }
  } catch {
    // localStorage can throw in locked-down webviews; nothing here is critical.
  }
}

// SPDX-License-Identifier: BUSL-1.1

import type { SecretPolicy } from './replay';

const STORAGE_KEY = 'qoredb_replay_secret_policy';
const CHANGED_EVENT = 'qoredb:replay-secret-policy-changed';

/**
 * Warn rather than redact by default.
 *
 * A replay set is versioned with its query text, so a credential inside one is
 * committed with it. Redacting removes the risk and the feature in the same
 * move — a redacted literal matches nothing, so the set stops replaying. The
 * default therefore flags what looks like a secret and leaves the call to the
 * user, who can drop the entry before saving.
 */
const DEFAULT_POLICY: SecretPolicy = 'warn';

export function getSecretPolicy(): SecretPolicy {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    return stored === 'off' || stored === 'redact' || stored === 'warn' ? stored : DEFAULT_POLICY;
  } catch {
    return DEFAULT_POLICY;
  }
}

export function setSecretPolicy(policy: SecretPolicy): void {
  try {
    if (policy === DEFAULT_POLICY) {
      localStorage.removeItem(STORAGE_KEY);
    } else {
      localStorage.setItem(STORAGE_KEY, policy);
    }
    window.dispatchEvent(new CustomEvent(CHANGED_EVENT));
  } catch {
    /* a rejected write only costs the persistence, not the session */
  }
}

export function subscribeSecretPolicy(handler: (policy: SecretPolicy) => void): () => void {
  const listener = () => handler(getSecretPolicy());
  window.addEventListener(CHANGED_EVENT, listener);
  return () => window.removeEventListener(CHANGED_EVENT, listener);
}

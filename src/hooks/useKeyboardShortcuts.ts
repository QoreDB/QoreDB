// SPDX-License-Identifier: Apache-2.0

import { useSyncExternalStore } from 'react';
import { resolveBindings, subscribeShortcutChanges } from '@/lib/shortcuts';

/**
 * Subscribe to user-defined chord overrides; bindings re-resolve whenever the
 * registry emits a change event. Used by `ShortcutProvider` to dispatch
 * window-level shortcuts and by the cheatsheet / Settings UI for display.
 */
export function useShortcutBindings() {
  return useSyncExternalStore(
    subscribeShortcutChanges,
    () => resolveBindings(),
    () => resolveBindings(),
  );
}

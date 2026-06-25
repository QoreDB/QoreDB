// SPDX-License-Identifier: Apache-2.0

import { chordSignature } from './match';
import type { KeyChord, ShortcutId } from './types';

/**
 * Find shortcut IDs whose effective chord clashes. The map is keyed by
 * shortcut ID and lists every other ID that shares its chord (excluding
 * itself). Pairs appear on both sides.
 */
export function detectConflicts(
  bindings: Record<ShortcutId, KeyChord>
): Record<string, ShortcutId[]> {
  const bySignature = new Map<string, ShortcutId[]>();

  for (const [id, chord] of Object.entries(bindings) as [ShortcutId, KeyChord][]) {
    const sig = chordSignature(chord);
    const existing = bySignature.get(sig);
    if (existing) {
      existing.push(id);
    } else {
      bySignature.set(sig, [id]);
    }
  }

  const conflicts: Record<string, ShortcutId[]> = {};
  for (const ids of bySignature.values()) {
    if (ids.length < 2) continue;
    for (const id of ids) {
      conflicts[id] = ids.filter(other => other !== id);
    }
  }
  return conflicts;
}

// SPDX-License-Identifier: Apache-2.0

import { SHORTCUT_DEFINITIONS } from './defaults';
import type { KeyChord, ShortcutDefinition, ShortcutId, ShortcutOverrides } from './types';

const STORAGE_KEY = 'qoredb_shortcut_overrides';
const EVENT = 'qoredb:shortcuts-updated';

let cached: ShortcutOverrides | null = null;
let cachedBindings: Record<ShortcutId, KeyChord> | null = null;

function readRaw(): ShortcutOverrides {
  if (cached) return cached;
  if (typeof window === 'undefined') return {};

  try {
    const raw = window.localStorage.getItem(STORAGE_KEY);
    if (!raw) {
      cached = {};
      return cached;
    }
    const parsed = JSON.parse(raw) as ShortcutOverrides;
    cached = parsed;
    return parsed;
  } catch {
    cached = {};
    return cached;
  }
}

function writeRaw(overrides: ShortcutOverrides): void {
  cached = overrides;
  cachedBindings = null;
  if (typeof window === 'undefined') return;
  window.localStorage.setItem(STORAGE_KEY, JSON.stringify(overrides));
  window.dispatchEvent(new CustomEvent(EVENT));
}

/** Effective chord = override if any, otherwise the definition default. */
export function resolveChord(def: ShortcutDefinition): KeyChord {
  return readRaw()[def.id] ?? def.defaultChord;
}

/**
 * Effective bindings keyed by ID. The result is cached and the same reference
 * is returned until overrides change — required so `useSyncExternalStore`
 * does not loop on every render.
 */
export function resolveBindings(): Record<ShortcutId, KeyChord> {
  if (cachedBindings) return cachedBindings;
  const result = {} as Record<ShortcutId, KeyChord>;
  for (const def of SHORTCUT_DEFINITIONS) {
    result[def.id] = resolveChord(def);
  }
  cachedBindings = result;
  return result;
}

export function setOverride(id: ShortcutId, chord: KeyChord): void {
  const overrides = { ...readRaw(), [id]: chord };
  writeRaw(overrides);
}

export function clearOverride(id: ShortcutId): void {
  const overrides = { ...readRaw() };
  delete overrides[id];
  writeRaw(overrides);
}

export function clearAllOverrides(): void {
  writeRaw({});
}

export function getOverrides(): ShortcutOverrides {
  return { ...readRaw() };
}

export function importOverrides(json: string): void {
  const parsed = JSON.parse(json) as ShortcutOverrides;
  // Drop unknown ids defensively
  const valid = {} as ShortcutOverrides;
  const knownIds = new Set(SHORTCUT_DEFINITIONS.map(d => d.id));
  for (const [id, chord] of Object.entries(parsed) as [ShortcutId, KeyChord][]) {
    if (
      knownIds.has(id) &&
      chord &&
      Array.isArray(chord.modifiers) &&
      typeof chord.key === 'string'
    ) {
      valid[id] = chord;
    }
  }
  writeRaw(valid);
}

export function exportOverrides(): string {
  return JSON.stringify(readRaw(), null, 2);
}

/**
 * Subscribe to override changes (cross-component reactivity). Listener fires
 * after any setOverride / clearOverride / clearAllOverrides / importOverrides.
 */
export function subscribeShortcutChanges(listener: () => void): () => void {
  if (typeof window === 'undefined') return () => {};
  window.addEventListener(EVENT, listener);
  return () => window.removeEventListener(EVENT, listener);
}

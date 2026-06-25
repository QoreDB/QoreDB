// SPDX-License-Identifier: Apache-2.0

import type { ChordModifier, KeyChord } from './types';

const IS_MAC = typeof navigator !== 'undefined' && /Mac|iPhone|iPad|iPod/.test(navigator.platform);

const SYMBOL_MAC: Record<ChordModifier, string> = {
  mod: '⌘',
  shift: '⇧',
  alt: '⌥',
  ctrl: '⌃',
};

const SYMBOL_OTHER: Record<ChordModifier, string> = {
  mod: 'Ctrl',
  shift: 'Shift',
  alt: 'Alt',
  ctrl: 'Ctrl',
};

const KEY_DISPLAY: Record<string, string> = {
  ' ': 'Space',
  Enter: '↵',
  Escape: 'Esc',
  ArrowUp: '↑',
  ArrowDown: '↓',
  ArrowLeft: '←',
  ArrowRight: '→',
  Backspace: '⌫',
  Delete: 'Del',
  Tab: '⇥',
};

const ORDER: ChordModifier[] = ['ctrl', 'mod', 'alt', 'shift'];

/** Pretty-print a chord for the cheatsheet / settings panel. */
export function formatChord(chord: KeyChord): string {
  const symbols = IS_MAC ? SYMBOL_MAC : SYMBOL_OTHER;
  const sep = IS_MAC ? '' : '+';

  const mods = ORDER.filter(m => chord.modifiers.includes(m)).map(m => symbols[m]);
  const key = formatKey(chord.key);

  if (mods.length === 0) return key;
  return `${mods.join(sep)}${sep}${key}`;
}

function formatKey(key: string): string {
  if (KEY_DISPLAY[key]) return KEY_DISPLAY[key];
  if (key.length === 1) return key.toUpperCase();
  return key;
}

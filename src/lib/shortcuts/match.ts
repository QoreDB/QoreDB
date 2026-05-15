// SPDX-License-Identifier: Apache-2.0

import type { ChordModifier, KeyChord } from './types';

/** Return true if `event` triggers `chord`. */
export function chordMatches(event: KeyboardEvent, chord: KeyChord): boolean {
  const wantsMod = chord.modifiers.includes('mod');
  const wantsCtrl = chord.modifiers.includes('ctrl');
  const wantsShift = chord.modifiers.includes('shift');
  const wantsAlt = chord.modifiers.includes('alt');

  // `mod` matches Cmd on macOS or Ctrl elsewhere. When `ctrl` is requested
  // explicitly, the user wants Ctrl regardless of platform.
  const hasModKey = event.metaKey || event.ctrlKey;
  if (wantsMod && !hasModKey) return false;
  if (!wantsMod && !wantsCtrl && hasModKey) return false;
  if (wantsCtrl && !event.ctrlKey) return false;

  if (wantsShift !== event.shiftKey) return false;
  if (wantsAlt !== event.altKey) return false;

  return normalizeKey(event.key) === normalizeKey(chord.key);
}

export function normalizeKey(key: string): string {
  if (key.length === 1) return key.toLowerCase();
  return key;
}

/** Stable serialization for equality / map keys. */
export function chordSignature(chord: KeyChord): string {
  const mods = [...chord.modifiers].sort().join('+');
  return `${mods}::${normalizeKey(chord.key)}`;
}

export function chordsEqual(a: KeyChord, b: KeyChord): boolean {
  return chordSignature(a) === chordSignature(b);
}

/**
 * Reject chord recordings that would shadow OS-level bindings the user
 * cannot recover from. Best-effort — the Linux desktop especially has too
 * many WM-specific bindings to enumerate exhaustively; we cover the most
 * common GNOME / KDE / i3 defaults.
 */
export function isSystemReserved(chord: KeyChord): boolean {
  const mods = new Set<ChordModifier>(chord.modifiers);
  const key = normalizeKey(chord.key);
  const hasMod = mods.has('mod');
  const hasCtrl = mods.has('ctrl');
  const hasShift = mods.has('shift');
  const hasAlt = mods.has('alt');

  // Cmd/Ctrl + Tab — task switcher (all platforms)
  if (hasMod && key === 'Tab') return true;
  // Cmd+Q / Ctrl+Q — quit on macOS, sometimes on Linux
  if (hasMod && !hasShift && !hasAlt && key === 'q') return true;
  // F11 / F12 — fullscreen / devtools
  if (mods.size === 0 && (key === 'F11' || key === 'F12')) return true;
  // Naked Enter / Space — too disruptive as window-level bindings
  if (mods.size === 0 && (key === 'Enter' || key === ' ')) return true;

  // --- Linux WM defaults (best-effort, varies by distro / WM) ---
  // Alt+Tab / Alt+Shift+Tab — workspace task switcher (GNOME/KDE/i3)
  if (hasAlt && !hasMod && !hasCtrl && key === 'Tab') return true;
  // Alt+F4 — close window (most Linux WMs + Windows)
  if (hasAlt && !hasMod && !hasCtrl && !hasShift && key === 'F4') return true;
  // Alt+F2 — run dialog (GNOME/KDE)
  if (hasAlt && !hasMod && !hasCtrl && !hasShift && key === 'F2') return true;
  // Ctrl+Alt+T — terminal (Ubuntu / GNOME default)
  if (hasCtrl && hasAlt && !hasMod && !hasShift && key === 't') return true;
  // Ctrl+Alt+L — lock screen (GNOME / Ubuntu)
  if (hasCtrl && hasAlt && !hasMod && !hasShift && key === 'l') return true;
  // Ctrl+Alt+Delete — system menu (most Linux desktops + Windows)
  if (hasCtrl && hasAlt && !hasMod && !hasShift && key === 'Delete') return true;
  // Ctrl+Alt+F1..F12 — TTY switch (Linux console)
  if (hasCtrl && hasAlt && !hasMod && !hasShift && /^F([1-9]|1[0-2])$/.test(key)) return true;
  // PrintScreen — screenshot on most desktops
  if (mods.size === 0 && key === 'PrintScreen') return true;

  return false;
}

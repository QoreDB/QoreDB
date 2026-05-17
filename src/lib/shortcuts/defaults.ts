// SPDX-License-Identifier: Apache-2.0

import type { ShortcutDefinition } from './types';

/**
 * Built-in shortcut catalogue.
 *
 * Adding a new shortcut here automatically surfaces it in the cheatsheet, the
 * Settings → Shortcuts panel, and the conflict detector. `useKeyboardShortcuts`
 * resolves callbacks by `id`.
 */
export const SHORTCUT_DEFINITIONS: ShortcutDefinition[] = [
  {
    id: 'search',
    category: 'general',
    labelKey: 'cheatsheet.search',
    defaultChord: { modifiers: ['mod'], key: 'k' },
    worksInTextInput: true,
  },
  {
    id: 'settings',
    category: 'general',
    labelKey: 'cheatsheet.settings',
    defaultChord: { modifiers: ['mod'], key: ',' },
  },
  {
    id: 'cheatsheet',
    category: 'general',
    labelKey: 'cheatsheet.thisDialog',
    defaultChord: { modifiers: [], key: '?' },
  },
  {
    id: 'escape',
    category: 'general',
    labelKey: 'cheatsheet.exitZenMode',
    defaultChord: { modifiers: [], key: 'Escape' },
  },
  {
    id: 'newQuery',
    category: 'tabs',
    labelKey: 'cheatsheet.newQuery',
    defaultChord: { modifiers: ['mod'], key: 't' },
  },
  {
    id: 'closeTab',
    category: 'tabs',
    labelKey: 'cheatsheet.closeTab',
    defaultChord: { modifiers: ['mod'], key: 'w' },
  },
  {
    id: 'newConnection',
    category: 'tabs',
    labelKey: 'cheatsheet.newConnection',
    defaultChord: { modifiers: ['mod'], key: 'n' },
  },
  {
    id: 'convertToNotebook',
    category: 'editor',
    labelKey: 'cheatsheet.convertToNotebook',
    defaultChord: { modifiers: ['mod', 'shift'], key: 'n' },
    worksInTextInput: true,
  },
  {
    id: 'openLibrary',
    category: 'editor',
    labelKey: 'cheatsheet.openLibrary',
    defaultChord: { modifiers: ['mod', 'shift'], key: 'l' },
  },
  {
    id: 'fulltextSearch',
    category: 'editor',
    labelKey: 'cheatsheet.fulltextSearch',
    defaultChord: { modifiers: ['mod', 'shift'], key: 'f' },
  },
];

// SPDX-License-Identifier: Apache-2.0

/**
 * Customizable Keyboard Shortcuts — type definitions.
 *
 * `mod` is a virtual modifier that resolves to `Cmd` on macOS and `Ctrl` on
 * Windows/Linux. `ctrl` is exposed separately so users who want to bind Ctrl
 * explicitly on macOS can do so.
 */

export type ShortcutCategory = 'general' | 'tabs' | 'editor';

export type ChordModifier = 'mod' | 'shift' | 'alt' | 'ctrl';

export interface KeyChord {
  /** Order does not matter — comparison is set-based. */
  modifiers: ChordModifier[];
  /**
   * Logical key. Single printable character is lowercased; named keys keep
   * their canonical form (`Enter`, `Escape`, `ArrowUp`, …).
   */
  key: string;
}

export type ShortcutId =
  | 'search'
  | 'settings'
  | 'cheatsheet'
  | 'escape'
  | 'newQuery'
  | 'closeTab'
  | 'newConnection'
  | 'convertToNotebook'
  | 'openLibrary'
  | 'fulltextSearch'
  | 'refreshData'
  | 'toggleSandbox';

export interface ShortcutDefinition {
  id: ShortcutId;
  category: ShortcutCategory;
  labelKey: string;
  defaultChord: KeyChord;
  /** Set true if the shortcut must trigger even inside text inputs. */
  worksInTextInput?: boolean;
}

export type ShortcutOverrides = Partial<Record<ShortcutId, KeyChord>>;

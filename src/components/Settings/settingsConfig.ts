import { Globe, Code2, Shield, Database, Keyboard, KeyRound, type LucideIcon } from 'lucide-react';

export type SettingsSectionId = 'general' | 'editor' | 'security' | 'data' | 'shortcuts' | 'license';

export interface SettingsSection {
  id: SettingsSectionId;
  labelKey: string;
  icon: LucideIcon;
  keywords: string[];
}

export const SETTINGS_SECTIONS: SettingsSection[] = [
  {
    id: 'general',
    labelKey: 'settings.sections.general',
    icon: Globe,
    keywords: ['language', 'theme', 'appearance', 'langue', 'thème', 'apparence'],
  },
  {
    id: 'editor',
    labelKey: 'settings.sections.editor',
    icon: Code2,
    keywords: ['sandbox', 'changes', 'panel', 'discard', 'modifications'],
  },
  {
    id: 'security',
    labelKey: 'settings.sections.security',
    icon: Shield,
    keywords: ['safety', 'policy', 'production', 'dangerous', 'interceptor', 'sécurité'],
  },
  {
    id: 'data',
    labelKey: 'settings.sections.data',
    icon: Database,
    keywords: [
      'diagnostics',
      'history',
      'logs',
      'backup',
      'export',
      'import',
      'transfer',
      'données',
    ],
  },
  {
    id: 'shortcuts',
    labelKey: 'settings.sections.shortcuts',
    icon: Keyboard,
    keywords: ['keyboard', 'shortcut', 'hotkey', 'raccourci', 'clavier'],
  },
  {
    id: 'license',
    labelKey: 'settings.sections.license',
    icon: KeyRound,
    keywords: ['license', 'licence', 'pro', 'tier', 'key', 'clé', 'activation'],
  },
];

export interface KeyboardShortcut {
  id: string;
  labelKey: string;
  keys: {
    mac: string;
    windows: string;
  };
  category: 'navigation' | 'editor' | 'general';
}

export const KEYBOARD_SHORTCUTS: KeyboardShortcut[] = [
  // Navigation
  {
    id: 'search',
    labelKey: 'settings.shortcuts.search',
    keys: { mac: '⌘K', windows: 'Ctrl+K' },
    category: 'navigation',
  },
  {
    id: 'newConnection',
    labelKey: 'settings.shortcuts.newConnection',
    keys: { mac: '⌘N', windows: 'Ctrl+N' },
    category: 'navigation',
  },
  {
    id: 'settings',
    labelKey: 'settings.shortcuts.settings',
    keys: { mac: '⌘,', windows: 'Ctrl+,' },
    category: 'navigation',
  },
  {
    id: 'library',
    labelKey: 'settings.shortcuts.library',
    keys: { mac: '⌘⇧L', windows: 'Ctrl+Shift+L' },
    category: 'navigation',
  },
  {
    id: 'fulltextSearch',
    labelKey: 'settings.shortcuts.fulltextSearch',
    keys: { mac: '⌘⇧F', windows: 'Ctrl+Shift+F' },
    category: 'navigation',
  },
  // Editor
  {
    id: 'newQuery',
    labelKey: 'settings.shortcuts.newQuery',
    keys: { mac: '⌘T', windows: 'Ctrl+T' },
    category: 'editor',
  },
  {
    id: 'runQuery',
    labelKey: 'settings.shortcuts.runQuery',
    keys: { mac: '⌘↵', windows: 'Ctrl+Enter' },
    category: 'editor',
  },
  {
    id: 'closeTab',
    labelKey: 'settings.shortcuts.closeTab',
    keys: { mac: '⌘W', windows: 'Ctrl+W' },
    category: 'editor',
  },
  // General
  {
    id: 'escape',
    labelKey: 'settings.shortcuts.escape',
    keys: { mac: 'Esc', windows: 'Esc' },
    category: 'general',
  },
];

export function getSectionById(id: SettingsSectionId): SettingsSection | undefined {
  return SETTINGS_SECTIONS.find(section => section.id === id);
}

export function filterSectionsBySearch(
  sections: SettingsSection[],
  query: string
): SettingsSection[] {
  if (!query.trim()) return sections;
  const lowerQuery = query.toLowerCase();
  return sections.filter(
    section =>
      section.keywords.some(kw => kw.toLowerCase().includes(lowerQuery)) ||
      section.id.toLowerCase().includes(lowerQuery)
  );
}

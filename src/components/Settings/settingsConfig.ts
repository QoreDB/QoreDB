// SPDX-License-Identifier: Apache-2.0

import {
  Code2,
  Database,
  Globe,
  Keyboard,
  KeyRound,
  type LucideIcon,
  Puzzle,
  Shield,
  Sparkles,
} from 'lucide-react';

export type SettingsSectionId =
  | 'general'
  | 'editor'
  | 'security'
  | 'data'
  | 'shortcuts'
  | 'plugins'
  | 'license'
  | 'ai';

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
    keywords: [
      'language',
      'theme',
      'appearance',
      'version',
      'update',
      'tabs',
      'tab groups',
      'connection',
      'onboarding',
      'tour',
      'tutorial',
      'welcome',
      'langue',
      'thème',
      'apparence',
      'mise à jour',
      'onglets',
      'groupes',
      'connexion',
      'visite',
      'tutoriel',
      'bienvenue',
    ],
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
      'share',
      'upload',
      'link',
      'partage',
      'données',
      'crash recovery',
      'drafts',
      'query drafts',
      'brouillons',
      'ttl',
      'time-travel',
      'time travel',
      'changelog',
      'historique',
    ],
  },
  {
    id: 'shortcuts',
    labelKey: 'settings.sections.shortcuts',
    icon: Keyboard,
    keywords: ['keyboard', 'shortcut', 'hotkey', 'raccourci', 'clavier'],
  },
  {
    id: 'plugins',
    labelKey: 'settings.sections.plugins',
    icon: Puzzle,
    keywords: [
      'plugin',
      'plugins',
      'extension',
      'addon',
      'snippet',
      'template',
      'theme',
      'extension',
      'thème',
      'modèle',
    ],
  },
  {
    id: 'license',
    labelKey: 'settings.sections.license',
    icon: KeyRound,
    keywords: ['license', 'licence', 'pro', 'tier', 'key', 'clé', 'activation'],
  },
  {
    id: 'ai',
    labelKey: 'settings.sections.ai',
    icon: Sparkles,
    keywords: [
      'ai',
      'openai',
      'anthropic',
      'mistral',
      'gemini',
      'google',
      'deepseek',
      'ollama',
      'llm',
      'intelligence',
      'artificielle',
      'assistant',
    ],
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

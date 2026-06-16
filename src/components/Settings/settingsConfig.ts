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
  ShieldCheck,
  Sparkles,
} from 'lucide-react';
import { isWeb, isWebAdmin } from '@/lib/transport';

export type SettingsSectionId =
  | 'general'
  | 'editor'
  | 'security'
  | 'data'
  | 'shortcuts'
  | 'plugins'
  | 'license'
  | 'ai'
  | 'admin';

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
      'thème',
      'modèle',
      'marketplace',
      'install plugin',
      'discover',
      'browse',
      'catalog',
      'catalogue',
      'install',
      'installer',
      'parcourir',
      'découvrir',
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
  {
    id: 'admin',
    labelKey: 'settings.sections.admin',
    icon: ShieldCheck,
    keywords: [
      'admin',
      'administration',
      'users',
      'user',
      'utilisateur',
      'utilisateurs',
      'password',
      'reset',
      'mot de passe',
      'réinitialiser',
      'role',
      'rôle',
    ],
  },
];

/**
 * Sections available in the current runtime. The `admin` section only exists in
 * the web server build and only for users with admin rights.
 */
export function availableSettingsSections(): SettingsSection[] {
  return SETTINGS_SECTIONS.filter(section => section.id !== 'admin' || (isWeb && isWebAdmin()));
}

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

// SPDX-License-Identifier: Apache-2.0

import { X } from 'lucide-react';
import { useEffect, useMemo } from 'react';
import { useTranslation } from 'react-i18next';
import { useShortcutBindings } from '@/hooks/useKeyboardShortcuts';
import {
  formatChord,
  SHORTCUT_DEFINITIONS,
  type ShortcutCategory,
  type ShortcutDefinition,
} from '@/lib/shortcuts';

interface KeyboardCheatsheetProps {
  open: boolean;
  onClose: () => void;
}

const CATEGORY_ORDER: ShortcutCategory[] = ['general', 'tabs', 'editor'];
const CATEGORY_KEY: Record<ShortcutCategory, string> = {
  general: 'cheatsheet.general',
  tabs: 'cheatsheet.tabs',
  editor: 'cheatsheet.queryEditor',
};

export function KeyboardCheatsheet({ open, onClose }: KeyboardCheatsheetProps) {
  const { t } = useTranslation();
  const bindings = useShortcutBindings();

  useEffect(() => {
    if (!open) return;
    const handler = (e: KeyboardEvent) => {
      if (e.key === 'Escape' || e.key === '?') {
        e.preventDefault();
        onClose();
      }
    };
    window.addEventListener('keydown', handler);
    return () => window.removeEventListener('keydown', handler);
  }, [open, onClose]);

  const sections = useMemo(() => {
    const grouped = new Map<ShortcutCategory, ShortcutDefinition[]>();
    for (const def of SHORTCUT_DEFINITIONS) {
      const existing = grouped.get(def.category) ?? [];
      existing.push(def);
      grouped.set(def.category, existing);
    }
    return CATEGORY_ORDER.filter(c => grouped.has(c)).map(category => ({
      category,
      title: t(CATEGORY_KEY[category]),
      shortcuts: grouped.get(category) ?? [],
    }));
  }, [t]);

  if (!open) return null;

  return (
    <>
      {/* biome-ignore lint/a11y/useKeyWithClickEvents: backdrop dismiss */}
      {/* biome-ignore lint/a11y/noStaticElementInteractions: backdrop overlay */}
      <div className="fixed inset-0 z-50 bg-black/50 animate-in fade-in" onClick={onClose} />
      <div
        role="dialog"
        aria-label={t('cheatsheet.title')}
        className="fixed top-1/2 left-1/2 -translate-x-1/2 -translate-y-1/2 z-50 bg-popover border border-border rounded-lg shadow-xl p-6 w-full max-w-md animate-in fade-in zoom-in-95"
      >
        <div className="flex items-center justify-between mb-4">
          <h2 className="text-sm font-semibold text-foreground">{t('cheatsheet.title')}</h2>
          <button
            type="button"
            onClick={onClose}
            className="p-1 rounded-md hover:bg-muted text-muted-foreground hover:text-foreground transition-colors"
          >
            <X size={16} />
          </button>
        </div>
        <div className="space-y-5">
          {sections.map(section => (
            <div key={section.category}>
              <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                {section.title}
              </h3>
              <div className="space-y-1">
                {section.shortcuts.map(def => (
                  <div key={def.id} className="flex items-center justify-between py-1">
                    <span className="text-sm text-foreground">{t(def.labelKey)}</span>
                    <kbd className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono bg-muted rounded border border-border text-muted-foreground">
                      {formatChord(bindings[def.id])}
                    </kbd>
                  </div>
                ))}
              </div>
            </div>
          ))}
        </div>
        <p className="mt-4 text-xs text-muted-foreground/60 text-center">
          {t('cheatsheet.pressToClose')}
        </p>
      </div>
    </>
  );
}

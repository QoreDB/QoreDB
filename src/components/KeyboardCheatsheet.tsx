// SPDX-License-Identifier: Apache-2.0

import { X } from 'lucide-react';
import { useEffect } from 'react';
import { useTranslation } from 'react-i18next';
import { getShortcutSymbol } from '@/utils/platform';

interface KeyboardCheatsheetProps {
  open: boolean;
  onClose: () => void;
}

export function KeyboardCheatsheet({ open, onClose }: KeyboardCheatsheetProps) {
  const { t } = useTranslation();
  const mod = getShortcutSymbol();

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

  if (!open) return null;

  const sections = [
    {
      title: t('cheatsheet.general'),
      shortcuts: [
        { keys: `${mod}+K`, label: t('cheatsheet.search') },
        { keys: `${mod}+,`, label: t('cheatsheet.settings') },
        { keys: '?', label: t('cheatsheet.thisDialog') },
        { keys: 'Esc', label: t('cheatsheet.exitZenMode') },
      ],
    },
    {
      title: t('cheatsheet.tabs'),
      shortcuts: [
        { keys: `${mod}+T`, label: t('cheatsheet.newQuery') },
        { keys: `${mod}+W`, label: t('cheatsheet.closeTab') },
        { keys: `${mod}+N`, label: t('cheatsheet.newConnection') },
      ],
    },
    {
      title: t('cheatsheet.queryEditor'),
      shortcuts: [
        { keys: `${mod}+Enter`, label: t('cheatsheet.executeQuery') },
        { keys: `${mod}+Shift+L`, label: t('cheatsheet.openLibrary') },
        { keys: `${mod}+Shift+F`, label: t('cheatsheet.fulltextSearch') },
      ],
    },
  ];

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
            <div key={section.title}>
              <h3 className="text-xs font-semibold text-muted-foreground uppercase tracking-wider mb-2">
                {section.title}
              </h3>
              <div className="space-y-1">
                {section.shortcuts.map(shortcut => (
                  <div key={shortcut.keys} className="flex items-center justify-between py-1">
                    <span className="text-sm text-foreground">{shortcut.label}</span>
                    <kbd className="inline-flex items-center gap-1 px-2 py-0.5 text-xs font-mono bg-muted rounded border border-border text-muted-foreground">
                      {shortcut.keys}
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

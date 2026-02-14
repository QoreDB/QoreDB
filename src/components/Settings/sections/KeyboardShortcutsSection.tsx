import { useTranslation } from 'react-i18next';
import { SettingsCard } from '../SettingsCard';
import { KEYBOARD_SHORTCUTS, type KeyboardShortcut } from '../settingsConfig';

interface KeyboardShortcutsSectionProps {
  searchQuery?: string;
}

function isMac(): boolean {
  return typeof navigator !== 'undefined' && /Mac|iPod|iPhone|iPad/.test(navigator.platform);
}

function ShortcutKey({ shortcut }: { shortcut: KeyboardShortcut }) {
  const key = isMac() ? shortcut.keys.mac : shortcut.keys.windows;

  return (
    <kbd className="inline-flex items-center px-2 py-0.5 text-xs font-mono bg-muted/70 border border-border/50 rounded">
      {key}
    </kbd>
  );
}

export function KeyboardShortcutsSection({ searchQuery }: KeyboardShortcutsSectionProps) {
  const { t } = useTranslation();

  const categories = [
    { id: 'navigation', labelKey: 'settings.shortcuts.categoryNavigation' },
    { id: 'editor', labelKey: 'settings.shortcuts.categoryEditor' },
    { id: 'general', labelKey: 'settings.shortcuts.categoryGeneral' },
  ] as const;

  return (
    <>
      {categories.map(category => {
        const shortcuts = KEYBOARD_SHORTCUTS.filter(s => s.category === category.id);
        if (shortcuts.length === 0) return null;

        return (
          <SettingsCard
            key={category.id}
            id={`shortcuts-${category.id}`}
            title={t(category.labelKey)}
            searchQuery={searchQuery}
          >
            <div className="space-y-2">
              {shortcuts.map(shortcut => (
                <div key={shortcut.id} className="flex items-center justify-between py-1 text-sm">
                  <span className="text-muted-foreground">{t(shortcut.labelKey)}</span>
                  <ShortcutKey shortcut={shortcut} />
                </div>
              ))}
            </div>
          </SettingsCard>
        );
      })}
    </>
  );
}
